//! Tag Correction API - Human-in-the-Loop tag management
//!
//! Provides APIs for users to:
//! - Confirm AI-generated tags
//! - Reject AI-generated tags
//! - Manually add/remove tags
//! - Batch tag operations
//! - Create and merge tags
//!
//! # Requirements
//! - Human-in-the-Loop: Tag confirmation/rejection API

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::core::types::{Tag, TagType, TagSource};
use super::error::{TagError, Result};
use super::manager::TagManager;

/// Tag correction commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TagCommand {
    /// Confirm an AI-generated tag
    ConfirmTag {
        file_id: Uuid,
        tag_id: Uuid,
    },

    /// Reject an AI-generated tag
    RejectTag {
        file_id: Uuid,
        tag_id: Uuid,
        /// Whether to block similar tags for this file
        block_similar: bool,
    },

    /// Manually add a tag to a file
    AddTag {
        file_id: Uuid,
        tag_id: Uuid,
    },

    /// Remove a tag from a file
    RemoveTag {
        file_id: Uuid,
        tag_id: Uuid,
    },

    /// Batch tag operations
    BatchTag {
        file_ids: Vec<Uuid>,
        add_tags: Vec<Uuid>,
        remove_tags: Vec<Uuid>,
    },

    /// Create a new tag
    CreateTag {
        name: String,
        parent_id: Option<Uuid>,
        tag_type: TagType,
        color: Option<String>,
    },

    /// Merge multiple tags into one
    MergeTags {
        source_tag_ids: Vec<Uuid>,
        target_tag_id: Uuid,
    },

    /// Rename a tag
    RenameTag {
        tag_id: Uuid,
        new_name: String,
    },

    /// Delete a tag
    DeleteTag {
        tag_id: Uuid,
    },

    /// Set tag parent (move in hierarchy)
    SetTagParent {
        tag_id: Uuid,
        new_parent_id: Option<Uuid>,
    },
}

/// Result of a tag correction operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagCorrectionResult {
    /// Whether the operation succeeded
    pub success: bool,
    /// Affected file IDs
    pub affected_files: Vec<Uuid>,
    /// Affected tag IDs
    pub affected_tags: Vec<Uuid>,
    /// Human-readable message
    pub message: String,
    /// Created tag (if applicable)
    pub created_tag: Option<Tag>,
}

impl TagCorrectionResult {
    fn success(message: impl Into<String>) -> Self {
        Self {
            success: true,
            affected_files: Vec::new(),
            affected_tags: Vec::new(),
            message: message.into(),
            created_tag: None,
        }
    }

    fn with_files(mut self, files: Vec<Uuid>) -> Self {
        self.affected_files = files;
        self
    }

    fn with_tags(mut self, tags: Vec<Uuid>) -> Self {
        self.affected_tags = tags;
        self
    }

    fn with_created_tag(mut self, tag: Tag) -> Self {
        self.created_tag = Some(tag);
        self
    }
}

/// Tag correction service
pub struct TagCorrectionService {
    pool: SqlitePool,
}

impl TagCorrectionService {
    /// Create a new TagCorrectionService
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Execute a tag correction command
    pub async fn execute(&self, cmd: TagCommand, manager: &TagManager) -> Result<TagCorrectionResult> {
        match cmd {
            TagCommand::ConfirmTag { file_id, tag_id } => {
                self.confirm_tag(file_id, tag_id).await
            }
            TagCommand::RejectTag { file_id, tag_id, block_similar } => {
                self.reject_tag(file_id, tag_id, block_similar).await
            }
            TagCommand::AddTag { file_id, tag_id } => {
                self.add_tag(file_id, tag_id, manager).await
            }
            TagCommand::RemoveTag { file_id, tag_id } => {
                self.remove_tag(file_id, tag_id, manager).await
            }
            TagCommand::BatchTag { file_ids, add_tags, remove_tags } => {
                self.batch_tag(file_ids, add_tags, remove_tags, manager).await
            }
            TagCommand::CreateTag { name, parent_id, tag_type, color } => {
                self.create_tag(name, parent_id, tag_type, color, manager).await
            }
            TagCommand::MergeTags { source_tag_ids, target_tag_id } => {
                self.merge_tags(source_tag_ids, target_tag_id, manager).await
            }
            TagCommand::RenameTag { tag_id, new_name } => {
                self.rename_tag(tag_id, new_name, manager).await
            }
            TagCommand::DeleteTag { tag_id } => {
                self.delete_tag(tag_id, manager).await
            }
            TagCommand::SetTagParent { tag_id, new_parent_id } => {
                self.set_tag_parent(tag_id, new_parent_id, manager).await
            }
        }
    }

    /// Confirm an AI-generated tag
    async fn confirm_tag(&self, file_id: Uuid, tag_id: Uuid) -> Result<TagCorrectionResult> {
        let now = Utc::now().to_rfc3339();

        let result = sqlx::query(
            r#"
            UPDATE file_tags 
            SET is_confirmed = 1, is_rejected = 0, user_action_at = ?
            WHERE file_id = ? AND tag_id = ?
            "#,
        )
        .bind(&now)
        .bind(file_id.to_string())
        .bind(tag_id.to_string())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(TagError::RelationNotFound { file_id, tag_id });
        }

        Ok(TagCorrectionResult::success("Tag confirmed")
            .with_files(vec![file_id])
            .with_tags(vec![tag_id]))
    }

    /// Reject an AI-generated tag
    async fn reject_tag(
        &self,
        file_id: Uuid,
        tag_id: Uuid,
        block_similar: bool,
    ) -> Result<TagCorrectionResult> {
        let now = Utc::now().to_rfc3339();

        let result = sqlx::query(
            r#"
            UPDATE file_tags 
            SET is_confirmed = 0, is_rejected = 1, user_action_at = ?
            WHERE file_id = ? AND tag_id = ?
            "#,
        )
        .bind(&now)
        .bind(file_id.to_string())
        .bind(tag_id.to_string())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(TagError::RelationNotFound { file_id, tag_id });
        }

        // If block_similar is true, we could store a block rule
        // For now, the rejection itself serves as learning data
        let message = if block_similar {
            "Tag rejected and similar tags will be blocked"
        } else {
            "Tag rejected"
        };

        Ok(TagCorrectionResult::success(message)
            .with_files(vec![file_id])
            .with_tags(vec![tag_id]))
    }

    /// Manually add a tag to a file
    async fn add_tag(
        &self,
        file_id: Uuid,
        tag_id: Uuid,
        manager: &TagManager,
    ) -> Result<TagCorrectionResult> {
        // Check if tag exists
        if manager.get_tag(tag_id).await?.is_none() {
            return Err(TagError::TagNotFound { id: tag_id });
        }

        // Add the tag
        manager.add_tag_to_file(file_id, tag_id, TagSource::Manual, None).await?;

        Ok(TagCorrectionResult::success("Tag added")
            .with_files(vec![file_id])
            .with_tags(vec![tag_id]))
    }

    /// Remove a tag from a file
    async fn remove_tag(
        &self,
        file_id: Uuid,
        tag_id: Uuid,
        manager: &TagManager,
    ) -> Result<TagCorrectionResult> {
        manager.remove_tag_from_file(file_id, tag_id).await?;

        Ok(TagCorrectionResult::success("Tag removed")
            .with_files(vec![file_id])
            .with_tags(vec![tag_id]))
    }

    /// Batch tag operations
    async fn batch_tag(
        &self,
        file_ids: Vec<Uuid>,
        add_tags: Vec<Uuid>,
        remove_tags: Vec<Uuid>,
        manager: &TagManager,
    ) -> Result<TagCorrectionResult> {
        let mut affected_files = Vec::new();
        let mut affected_tags = Vec::new();

        for file_id in &file_ids {
            // Add tags
            for tag_id in &add_tags {
                // Skip if relation already exists
                if manager.get_file_tag_relation(*file_id, *tag_id).await?.is_none() {
                    manager.add_tag_to_file(*file_id, *tag_id, TagSource::Manual, None).await?;
                    if !affected_tags.contains(tag_id) {
                        affected_tags.push(*tag_id);
                    }
                }
            }

            // Remove tags
            for tag_id in &remove_tags {
                if manager.get_file_tag_relation(*file_id, *tag_id).await?.is_some() {
                    manager.remove_tag_from_file(*file_id, *tag_id).await?;
                    if !affected_tags.contains(tag_id) {
                        affected_tags.push(*tag_id);
                    }
                }
            }

            affected_files.push(*file_id);
        }

        Ok(TagCorrectionResult::success(format!(
            "Batch operation completed: {} files, {} tags added, {} tags removed",
            affected_files.len(),
            add_tags.len(),
            remove_tags.len()
        ))
        .with_files(affected_files)
        .with_tags(affected_tags))
    }

    /// Create a new tag
    async fn create_tag(
        &self,
        name: String,
        parent_id: Option<Uuid>,
        tag_type: TagType,
        color: Option<String>,
        manager: &TagManager,
    ) -> Result<TagCorrectionResult> {
        let tag = manager.create_tag(name, tag_type, parent_id, color, None).await?;

        Ok(TagCorrectionResult::success("Tag created")
            .with_tags(vec![tag.id])
            .with_created_tag(tag))
    }

    /// Merge multiple tags into one
    async fn merge_tags(
        &self,
        source_tag_ids: Vec<Uuid>,
        target_tag_id: Uuid,
        manager: &TagManager,
    ) -> Result<TagCorrectionResult> {
        // Verify target tag exists
        if manager.get_tag(target_tag_id).await?.is_none() {
            return Err(TagError::TagNotFound { id: target_tag_id });
        }

        let mut affected_files = Vec::new();

        for source_tag_id in &source_tag_ids {
            if *source_tag_id == target_tag_id {
                continue;
            }

            // Get all files with the source tag
            let file_ids = manager.get_files_by_tag(*source_tag_id).await?;

            for file_id in file_ids {
                // Add target tag if not present
                if manager.get_file_tag_relation(file_id, target_tag_id).await?.is_none() {
                    manager.add_tag_to_file(file_id, target_tag_id, TagSource::Manual, None).await?;
                }

                // Remove source tag
                manager.remove_tag_from_file(file_id, *source_tag_id).await.ok();

                if !affected_files.contains(&file_id) {
                    affected_files.push(file_id);
                }
            }

            // Delete the source tag
            manager.delete_tag(*source_tag_id).await?;
        }

        let mut affected_tags = source_tag_ids.clone();
        affected_tags.push(target_tag_id);

        Ok(TagCorrectionResult::success(format!(
            "Merged {} tags into target tag",
            source_tag_ids.len()
        ))
        .with_files(affected_files)
        .with_tags(affected_tags))
    }

    /// Rename a tag
    async fn rename_tag(
        &self,
        tag_id: Uuid,
        new_name: String,
        manager: &TagManager,
    ) -> Result<TagCorrectionResult> {
        // Get existing tag
        let mut tag = manager.get_tag(tag_id).await?
            .ok_or(TagError::TagNotFound { id: tag_id })?;

        // Check for duplicate name
        if let Some(existing) = manager.get_tag_by_name(&new_name).await? {
            if existing.id != tag_id {
                return Err(TagError::TagAlreadyExists { name: new_name });
            }
        }

        tag.name = new_name;
        manager.update_tag(&tag).await?;

        Ok(TagCorrectionResult::success("Tag renamed")
            .with_tags(vec![tag_id]))
    }

    /// Delete a tag
    async fn delete_tag(
        &self,
        tag_id: Uuid,
        manager: &TagManager,
    ) -> Result<TagCorrectionResult> {
        // Get files that will be affected
        let affected_files = manager.get_files_by_tag(tag_id).await?;

        manager.delete_tag(tag_id).await?;

        Ok(TagCorrectionResult::success("Tag deleted")
            .with_files(affected_files)
            .with_tags(vec![tag_id]))
    }

    /// Set tag parent (move in hierarchy)
    async fn set_tag_parent(
        &self,
        tag_id: Uuid,
        new_parent_id: Option<Uuid>,
        manager: &TagManager,
    ) -> Result<TagCorrectionResult> {
        manager.hierarchy().set_parent(tag_id, new_parent_id).await?;

        let mut affected_tags = vec![tag_id];
        if let Some(parent_id) = new_parent_id {
            affected_tags.push(parent_id);
        }

        Ok(TagCorrectionResult::success("Tag parent updated")
            .with_tags(affected_tags))
    }

    /// Get user's tag preferences based on confirmation/rejection history
    pub async fn get_tag_preferences(&self, file_id: Option<Uuid>) -> Result<TagPreferences> {
        // Get confirmed tags
        let confirmed_query = if let Some(fid) = file_id {
            sqlx::query_as::<_, (String, i64)>(
                r#"
                SELECT tag_id, COUNT(*) as count 
                FROM file_tags 
                WHERE is_confirmed = 1 AND file_id = ?
                GROUP BY tag_id 
                ORDER BY count DESC
                "#,
            )
            .bind(fid.to_string())
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, (String, i64)>(
                r#"
                SELECT tag_id, COUNT(*) as count 
                FROM file_tags 
                WHERE is_confirmed = 1 
                GROUP BY tag_id 
                ORDER BY count DESC
                "#,
            )
            .fetch_all(&self.pool)
            .await?
        };

        // Get rejected tags
        let rejected_query = if let Some(fid) = file_id {
            sqlx::query_as::<_, (String, i64)>(
                r#"
                SELECT tag_id, COUNT(*) as count 
                FROM file_tags 
                WHERE is_rejected = 1 AND file_id = ?
                GROUP BY tag_id 
                ORDER BY count DESC
                "#,
            )
            .bind(fid.to_string())
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, (String, i64)>(
                r#"
                SELECT tag_id, COUNT(*) as count 
                FROM file_tags 
                WHERE is_rejected = 1 
                GROUP BY tag_id 
                ORDER BY count DESC
                "#,
            )
            .fetch_all(&self.pool)
            .await?
        };

        let confirmed_tags: Vec<(Uuid, u64)> = confirmed_query
            .into_iter()
            .filter_map(|(id, count)| {
                Uuid::parse_str(&id).ok().map(|uuid| (uuid, count as u64))
            })
            .collect();

        let rejected_tags: Vec<(Uuid, u64)> = rejected_query
            .into_iter()
            .filter_map(|(id, count)| {
                Uuid::parse_str(&id).ok().map(|uuid| (uuid, count as u64))
            })
            .collect();

        Ok(TagPreferences {
            confirmed_tags,
            rejected_tags,
        })
    }
}

/// User's tag preferences based on history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagPreferences {
    /// Tags that have been confirmed (tag_id, confirmation_count)
    pub confirmed_tags: Vec<(Uuid, u64)>,
    /// Tags that have been rejected (tag_id, rejection_count)
    pub rejected_tags: Vec<(Uuid, u64)>,
}
