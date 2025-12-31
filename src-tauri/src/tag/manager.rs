//! Tag Manager - Core tag management functionality
//!
//! Provides automatic tag generation and tag CRUD operations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;
use uuid::Uuid;

use crate::core::types::{Tag, TagType, FileTagRelation, TagSource};
use super::error::{TagError, Result};
use super::hierarchy::TagHierarchy;
use super::sensitive::{SensitiveTagDetector, SensitivityLevel};

/// Configuration for the TagManager
#[derive(Debug, Clone)]
pub struct TagManagerConfig {
    /// Minimum confidence threshold for auto-generated tags
    pub min_confidence: f32,
    /// Maximum number of auto-generated tags per file
    pub max_auto_tags: usize,
    /// Whether to enable sensitive tag detection
    pub enable_sensitive_detection: bool,
    /// Default color for new tags
    pub default_color: String,
}

impl Default for TagManagerConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.5,
            max_auto_tags: 5,
            enable_sensitive_detection: true,
            default_color: "#808080".to_string(),
        }
    }
}

/// Result of automatic tag generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoTagResult {
    /// File ID that was tagged
    pub file_id: Uuid,
    /// Tags that were assigned
    pub assigned_tags: Vec<TagSuggestion>,
    /// Tags that require user confirmation (sensitive)
    pub pending_confirmation: Vec<TagSuggestion>,
    /// Total processing time in milliseconds
    pub duration_ms: u64,
}

/// A suggested tag with confidence score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagSuggestion {
    /// The suggested tag
    pub tag: Tag,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Reason for the suggestion
    pub reason: String,
    /// Whether this tag requires user confirmation
    pub requires_confirmation: bool,
}

/// Tag Manager - handles all tag operations
pub struct TagManager {
    pool: SqlitePool,
    config: TagManagerConfig,
    hierarchy: TagHierarchy,
    sensitive_detector: SensitiveTagDetector,
}

impl TagManager {
    /// Create a new TagManager
    pub async fn new(pool: SqlitePool, config: TagManagerConfig) -> Result<Self> {
        let hierarchy = TagHierarchy::new(pool.clone()).await?;
        let sensitive_detector = SensitiveTagDetector::new();
        
        Ok(Self {
            pool,
            config,
            hierarchy,
            sensitive_detector,
        })
    }

    /// Get the tag hierarchy
    pub fn hierarchy(&self) -> &TagHierarchy {
        &self.hierarchy
    }

    /// Get a mutable reference to the tag hierarchy
    pub fn hierarchy_mut(&mut self) -> &mut TagHierarchy {
        &mut self.hierarchy
    }

    // ========================================================================
    // Tag CRUD Operations
    // ========================================================================

    /// Create a new tag
    pub async fn create_tag(
        &self,
        name: String,
        tag_type: TagType,
        parent_id: Option<Uuid>,
        color: Option<String>,
        icon: Option<String>,
    ) -> Result<Tag> {
        // Validate tag name
        self.validate_tag_name(&name)?;

        // Check for duplicate
        if self.get_tag_by_name(&name).await?.is_some() {
            return Err(TagError::TagAlreadyExists { name });
        }

        // Validate hierarchy depth if parent is specified
        if let Some(pid) = parent_id {
            let depth = self.hierarchy.get_depth(pid).await?;
            if depth >= 2 {
                // Max 3 levels (0, 1, 2)
                return Err(TagError::HierarchyDepthExceeded { max_depth: 3 });
            }
        }

        let tag = Tag {
            id: Uuid::now_v7(),
            name: name.clone(),
            display_name: HashMap::new(),
            parent_id,
            tag_type,
            color: color.unwrap_or_else(|| self.config.default_color.clone()),
            icon,
            is_system: false,
            created_at: Utc::now(),
            usage_count: 0,
        };

        // Insert into database
        let display_name_json = serde_json::to_string(&tag.display_name)
            .map_err(|e| TagError::Internal(e.to_string()))?;
        let tag_type_str = format!("{:?}", tag.tag_type);
        let parent_id_str = tag.parent_id.map(|id| id.to_string());

        sqlx::query(
            r#"
            INSERT INTO tags (id, name, display_name, parent_id, tag_type, color, icon, is_system, created_at, usage_count)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(tag.id.to_string())
        .bind(&tag.name)
        .bind(&display_name_json)
        .bind(&parent_id_str)
        .bind(&tag_type_str)
        .bind(&tag.color)
        .bind(&tag.icon)
        .bind(tag.is_system)
        .bind(tag.created_at.to_rfc3339())
        .bind(tag.usage_count as i64)
        .execute(&self.pool)
        .await?;

        // Refresh hierarchy cache
        self.hierarchy.refresh().await?;

        Ok(tag)
    }

    /// Get a tag by ID
    pub async fn get_tag(&self, id: Uuid) -> Result<Option<Tag>> {
        let row = sqlx::query_as::<_, TagRow>(
            "SELECT id, name, display_name, parent_id, tag_type, color, icon, is_system, created_at, usage_count FROM tags WHERE id = ?"
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| r.into_tag()).transpose()
    }

    /// Get a tag by name
    pub async fn get_tag_by_name(&self, name: &str) -> Result<Option<Tag>> {
        let row = sqlx::query_as::<_, TagRow>(
            "SELECT id, name, display_name, parent_id, tag_type, color, icon, is_system, created_at, usage_count FROM tags WHERE name = ?"
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| r.into_tag()).transpose()
    }

    /// Get all tags
    pub async fn get_all_tags(&self) -> Result<Vec<Tag>> {
        let rows = sqlx::query_as::<_, TagRow>(
            "SELECT id, name, display_name, parent_id, tag_type, color, icon, is_system, created_at, usage_count FROM tags ORDER BY usage_count DESC"
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(|r| r.into_tag()).collect()
    }

    /// Update a tag
    pub async fn update_tag(&self, tag: &Tag) -> Result<()> {
        let display_name_json = serde_json::to_string(&tag.display_name)
            .map_err(|e| TagError::Internal(e.to_string()))?;
        let tag_type_str = format!("{:?}", tag.tag_type);
        let parent_id_str = tag.parent_id.map(|id| id.to_string());

        sqlx::query(
            r#"
            UPDATE tags SET name = ?, display_name = ?, parent_id = ?, tag_type = ?, 
                           color = ?, icon = ?, usage_count = ?
            WHERE id = ?
            "#,
        )
        .bind(&tag.name)
        .bind(&display_name_json)
        .bind(&parent_id_str)
        .bind(&tag_type_str)
        .bind(&tag.color)
        .bind(&tag.icon)
        .bind(tag.usage_count as i64)
        .bind(tag.id.to_string())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Delete a tag
    pub async fn delete_tag(&self, id: Uuid) -> Result<()> {
        // Check if it's a system tag
        if let Some(tag) = self.get_tag(id).await? {
            if tag.is_system {
                return Err(TagError::CannotDeleteSystemTag { name: tag.name });
            }
        }

        // Delete the tag (cascade will handle file_tags)
        sqlx::query("DELETE FROM tags WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        // Refresh hierarchy cache
        self.hierarchy.refresh().await?;

        Ok(())
    }

    // ========================================================================
    // File-Tag Relations
    // ========================================================================

    /// Add a tag to a file
    pub async fn add_tag_to_file(
        &self,
        file_id: Uuid,
        tag_id: Uuid,
        source: TagSource,
        confidence: Option<f32>,
    ) -> Result<FileTagRelation> {
        // Check if relation already exists
        if self.get_file_tag_relation(file_id, tag_id).await?.is_some() {
            return Err(TagError::RelationAlreadyExists { file_id, tag_id });
        }

        let relation = FileTagRelation {
            id: Uuid::now_v7(),
            file_id,
            tag_id,
            source,
            confidence,
            is_confirmed: matches!(source, TagSource::Manual),
            is_rejected: false,
            created_at: Utc::now(),
            user_action_at: if matches!(source, TagSource::Manual) {
                Some(Utc::now())
            } else {
                None
            },
        };

        let source_str = format!("{:?}", relation.source);

        sqlx::query(
            r#"
            INSERT INTO file_tags (id, file_id, tag_id, source, confidence, is_confirmed, is_rejected, created_at, user_action_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(relation.id.to_string())
        .bind(relation.file_id.to_string())
        .bind(relation.tag_id.to_string())
        .bind(&source_str)
        .bind(relation.confidence)
        .bind(relation.is_confirmed)
        .bind(relation.is_rejected)
        .bind(relation.created_at.to_rfc3339())
        .bind(relation.user_action_at.map(|t| t.to_rfc3339()))
        .execute(&self.pool)
        .await?;

        // Increment usage count
        sqlx::query("UPDATE tags SET usage_count = usage_count + 1 WHERE id = ?")
            .bind(tag_id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(relation)
    }

    /// Remove a tag from a file
    pub async fn remove_tag_from_file(&self, file_id: Uuid, tag_id: Uuid) -> Result<()> {
        let result = sqlx::query("DELETE FROM file_tags WHERE file_id = ? AND tag_id = ?")
            .bind(file_id.to_string())
            .bind(tag_id.to_string())
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(TagError::RelationNotFound { file_id, tag_id });
        }

        // Decrement usage count
        sqlx::query("UPDATE tags SET usage_count = MAX(0, usage_count - 1) WHERE id = ?")
            .bind(tag_id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Get all tags for a file
    pub async fn get_file_tags(&self, file_id: Uuid) -> Result<Vec<FileTagRelation>> {
        let rows = sqlx::query_as::<_, FileTagRelationRow>(
            r#"
            SELECT id, file_id, tag_id, source, confidence, is_confirmed, is_rejected, created_at, user_action_at
            FROM file_tags WHERE file_id = ?
            "#,
        )
        .bind(file_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(|r| r.into_relation()).collect()
    }

    /// Get all files with a specific tag
    pub async fn get_files_by_tag(&self, tag_id: Uuid) -> Result<Vec<Uuid>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT file_id FROM file_tags WHERE tag_id = ? AND is_rejected = 0"
        )
        .bind(tag_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|(id,)| Uuid::parse_str(&id).map_err(|e| TagError::Internal(e.to_string())))
            .collect()
    }

    /// Get a specific file-tag relation
    pub async fn get_file_tag_relation(
        &self,
        file_id: Uuid,
        tag_id: Uuid,
    ) -> Result<Option<FileTagRelation>> {
        let row = sqlx::query_as::<_, FileTagRelationRow>(
            r#"
            SELECT id, file_id, tag_id, source, confidence, is_confirmed, is_rejected, created_at, user_action_at
            FROM file_tags WHERE file_id = ? AND tag_id = ?
            "#,
        )
        .bind(file_id.to_string())
        .bind(tag_id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| r.into_relation()).transpose()
    }

    // ========================================================================
    // Auto-tagging
    // ========================================================================

    /// Generate automatic tags for a file based on content analysis
    ///
    /// This is a simplified implementation that assigns tags based on file metadata.
    /// In a full implementation, this would use the embedding engine and ML models.
    pub async fn auto_tag_file(
        &self,
        file_id: Uuid,
        filename: &str,
        extension: &str,
        content_preview: Option<&str>,
    ) -> Result<AutoTagResult> {
        let start = std::time::Instant::now();
        let mut assigned_tags = Vec::new();
        let mut pending_confirmation = Vec::new();

        // Get or create file type tag
        let file_type_tag = self.get_or_create_file_type_tag(extension).await?;
        let file_type_suggestion = TagSuggestion {
            tag: file_type_tag.clone(),
            confidence: 1.0,
            reason: format!("File extension: .{}", extension),
            requires_confirmation: false,
        };

        // Add file type tag
        if self.get_file_tag_relation(file_id, file_type_tag.id).await?.is_none() {
            self.add_tag_to_file(file_id, file_type_tag.id, TagSource::AIGenerated, Some(1.0)).await?;
        }
        assigned_tags.push(file_type_suggestion);

        // Analyze content for additional tags
        if let Some(content) = content_preview {
            let content_tags = self.analyze_content_for_tags(content).await?;
            
            for suggestion in content_tags {
                // Check if tag requires confirmation (sensitive)
                let requires_confirmation = if self.config.enable_sensitive_detection {
                    self.sensitive_detector.check_sensitivity(&suggestion.tag.name) != SensitivityLevel::None
                } else {
                    false
                };

                if requires_confirmation {
                    pending_confirmation.push(TagSuggestion {
                        requires_confirmation: true,
                        ..suggestion
                    });
                } else if suggestion.confidence >= self.config.min_confidence {
                    // Add tag to file
                    if self.get_file_tag_relation(file_id, suggestion.tag.id).await?.is_none() {
                        self.add_tag_to_file(
                            file_id,
                            suggestion.tag.id,
                            TagSource::AIGenerated,
                            Some(suggestion.confidence),
                        ).await?;
                    }
                    assigned_tags.push(suggestion);
                }

                // Limit number of auto-tags
                if assigned_tags.len() >= self.config.max_auto_tags {
                    break;
                }
            }
        }

        Ok(AutoTagResult {
            file_id,
            assigned_tags,
            pending_confirmation,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Get or create a file type tag based on extension
    async fn get_or_create_file_type_tag(&self, extension: &str) -> Result<Tag> {
        let tag_name = self.extension_to_tag_name(extension);
        
        if let Some(tag) = self.get_tag_by_name(&tag_name).await? {
            return Ok(tag);
        }

        // Create new file type tag
        self.create_tag(
            tag_name,
            TagType::FileType,
            None,
            Some(self.extension_to_color(extension)),
            Some(self.extension_to_icon(extension)),
        ).await
    }

    /// Convert file extension to tag name
    fn extension_to_tag_name(&self, extension: &str) -> String {
        match extension.to_lowercase().as_str() {
            "pdf" => "Documents".to_string(),
            "doc" | "docx" => "Documents".to_string(),
            "xls" | "xlsx" => "Spreadsheets".to_string(),
            "ppt" | "pptx" => "Presentations".to_string(),
            "txt" | "md" => "Text".to_string(),
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" => "Images".to_string(),
            "mp4" | "avi" | "mov" | "mkv" => "Videos".to_string(),
            "mp3" | "wav" | "flac" | "ogg" => "Audio".to_string(),
            "rs" | "py" | "js" | "ts" | "java" | "cpp" | "c" | "go" => "Code".to_string(),
            "zip" | "rar" | "7z" | "tar" | "gz" => "Archives".to_string(),
            _ => "Other".to_string(),
        }
    }

    /// Get color for file type
    fn extension_to_color(&self, extension: &str) -> String {
        match extension.to_lowercase().as_str() {
            "pdf" | "doc" | "docx" => "#E53935".to_string(), // Red
            "xls" | "xlsx" => "#43A047".to_string(), // Green
            "ppt" | "pptx" => "#FB8C00".to_string(), // Orange
            "txt" | "md" => "#757575".to_string(), // Gray
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" => "#8E24AA".to_string(), // Purple
            "mp4" | "avi" | "mov" | "mkv" => "#D81B60".to_string(), // Pink
            "mp3" | "wav" | "flac" | "ogg" => "#00ACC1".to_string(), // Cyan
            "rs" | "py" | "js" | "ts" | "java" | "cpp" | "c" | "go" => "#1E88E5".to_string(), // Blue
            "zip" | "rar" | "7z" | "tar" | "gz" => "#6D4C41".to_string(), // Brown
            _ => "#808080".to_string(), // Default gray
        }
    }

    /// Get icon for file type
    fn extension_to_icon(&self, extension: &str) -> String {
        match extension.to_lowercase().as_str() {
            "pdf" | "doc" | "docx" => "📄".to_string(),
            "xls" | "xlsx" => "📊".to_string(),
            "ppt" | "pptx" => "📽️".to_string(),
            "txt" | "md" => "📝".to_string(),
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" => "🖼️".to_string(),
            "mp4" | "avi" | "mov" | "mkv" => "🎬".to_string(),
            "mp3" | "wav" | "flac" | "ogg" => "🎵".to_string(),
            "rs" | "py" | "js" | "ts" | "java" | "cpp" | "c" | "go" => "💻".to_string(),
            "zip" | "rar" | "7z" | "tar" | "gz" => "📦".to_string(),
            _ => "📁".to_string(),
        }
    }

    /// Analyze content for potential tags
    ///
    /// This is a simplified keyword-based implementation.
    /// A full implementation would use ML models for semantic analysis.
    async fn analyze_content_for_tags(&self, content: &str) -> Result<Vec<TagSuggestion>> {
        let mut suggestions = Vec::new();
        let content_lower = content.to_lowercase();

        // Simple keyword matching for common categories
        let keyword_tags = [
            (vec!["work", "project", "meeting", "deadline", "report"], "Work", 0.7),
            (vec!["personal", "family", "vacation", "birthday"], "Personal", 0.7),
            (vec!["study", "learn", "course", "tutorial", "education"], "Study", 0.7),
            (vec!["finance", "budget", "invoice", "payment", "expense"], "Finance", 0.8),
            (vec!["health", "medical", "doctor", "prescription"], "Health", 0.8),
            (vec!["travel", "flight", "hotel", "booking"], "Travel", 0.7),
        ];

        for (keywords, tag_name, base_confidence) in keyword_tags {
            let matches: usize = keywords.iter()
                .filter(|kw| content_lower.contains(*kw))
                .count();
            
            if matches > 0 {
                let confidence = (base_confidence + (matches as f32 * 0.05)).min(1.0);
                
                // Get or create the tag
                let tag = if let Some(existing) = self.get_tag_by_name(tag_name).await? {
                    existing
                } else {
                    self.create_tag(
                        tag_name.to_string(),
                        TagType::Category,
                        None,
                        None,
                        None,
                    ).await?
                };

                suggestions.push(TagSuggestion {
                    tag,
                    confidence,
                    reason: format!("Content contains keywords: {}", 
                        keywords.iter()
                            .filter(|kw| content_lower.contains(*kw))
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")),
                    requires_confirmation: false,
                });
            }
        }

        // Sort by confidence
        suggestions.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));

        Ok(suggestions)
    }

    /// Validate tag name
    fn validate_tag_name(&self, name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(TagError::InvalidTagName {
                reason: "Tag name cannot be empty".to_string(),
            });
        }
        if name.len() > 100 {
            return Err(TagError::InvalidTagName {
                reason: "Tag name cannot exceed 100 characters".to_string(),
            });
        }
        Ok(())
    }

    /// Get tag suggestions for a file (without applying them)
    pub async fn suggest_tags(
        &self,
        _file_id: Uuid,
        filename: &str,
        extension: &str,
        content_preview: Option<&str>,
    ) -> Result<Vec<TagSuggestion>> {
        let mut suggestions = Vec::new();

        // File type suggestion
        let file_type_tag = self.get_or_create_file_type_tag(extension).await?;
        suggestions.push(TagSuggestion {
            tag: file_type_tag,
            confidence: 1.0,
            reason: format!("File extension: .{}", extension),
            requires_confirmation: false,
        });

        // Content-based suggestions
        if let Some(content) = content_preview {
            let content_suggestions = self.analyze_content_for_tags(content).await?;
            suggestions.extend(content_suggestions);
        }

        // Check for sensitive tags
        if self.config.enable_sensitive_detection {
            for suggestion in &mut suggestions {
                let sensitivity = self.sensitive_detector.check_sensitivity(&suggestion.tag.name);
                if sensitivity != SensitivityLevel::None {
                    suggestion.requires_confirmation = true;
                }
            }
        }

        Ok(suggestions)
    }
}

// ============================================================================
// Database Row Types
// ============================================================================

#[derive(sqlx::FromRow)]
struct TagRow {
    id: String,
    name: String,
    display_name: Option<String>,
    parent_id: Option<String>,
    tag_type: String,
    color: String,
    icon: Option<String>,
    is_system: bool,
    created_at: String,
    usage_count: i64,
}

impl TagRow {
    fn into_tag(self) -> Result<Tag> {
        let display_name: HashMap<String, String> = self.display_name
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        let tag_type = match self.tag_type.as_str() {
            "Category" => TagType::Category,
            "FileType" => TagType::FileType,
            "Project" => TagType::Project,
            "Status" => TagType::Status,
            "Custom" => TagType::Custom,
            "AutoGenerated" => TagType::AutoGenerated,
            _ => TagType::Custom,
        };

        Ok(Tag {
            id: Uuid::parse_str(&self.id).map_err(|e| TagError::Internal(e.to_string()))?,
            name: self.name,
            display_name,
            parent_id: self.parent_id
                .as_deref()
                .map(|s| Uuid::parse_str(s))
                .transpose()
                .map_err(|e| TagError::Internal(e.to_string()))?,
            tag_type,
            color: self.color,
            icon: self.icon,
            is_system: self.is_system,
            created_at: DateTime::parse_from_rfc3339(&self.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| TagError::Internal(e.to_string()))?,
            usage_count: self.usage_count as u64,
        })
    }
}

#[derive(sqlx::FromRow)]
struct FileTagRelationRow {
    id: String,
    file_id: String,
    tag_id: String,
    source: String,
    confidence: Option<f64>,
    is_confirmed: bool,
    is_rejected: bool,
    created_at: String,
    user_action_at: Option<String>,
}

impl FileTagRelationRow {
    fn into_relation(self) -> Result<FileTagRelation> {
        let source = match self.source.as_str() {
            "Manual" => TagSource::Manual,
            "AIGenerated" => TagSource::AIGenerated,
            "Inherited" => TagSource::Inherited,
            "Imported" => TagSource::Imported,
            _ => TagSource::Manual,
        };

        Ok(FileTagRelation {
            id: Uuid::parse_str(&self.id).map_err(|e| TagError::Internal(e.to_string()))?,
            file_id: Uuid::parse_str(&self.file_id).map_err(|e| TagError::Internal(e.to_string()))?,
            tag_id: Uuid::parse_str(&self.tag_id).map_err(|e| TagError::Internal(e.to_string()))?,
            source,
            confidence: self.confidence.map(|c| c as f32),
            is_confirmed: self.is_confirmed,
            is_rejected: self.is_rejected,
            created_at: DateTime::parse_from_rfc3339(&self.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| TagError::Internal(e.to_string()))?,
            user_action_at: self.user_action_at
                .as_deref()
                .map(|s| DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&Utc)))
                .transpose()
                .map_err(|e| TagError::Internal(e.to_string()))?,
        })
    }
}
