//! Logic Chain Engine - Core relation management functionality
//!
//! Provides content similarity-based file associations and relation CRUD operations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::core::types::{FileRelation, RelationType, RelationSource, UserFeedback};
use crate::vector::{VectorStore, SearchFilter};
use super::error::{RelationError, Result};
use super::block_rules::BlockRuleStore;
use super::session::SessionTracker;

/// Configuration for the LogicChainEngine
#[derive(Debug, Clone)]
pub struct LogicChainConfig {
    /// Minimum similarity threshold for content-based relations (0.0 - 1.0)
    pub min_similarity_threshold: f32,
    /// Maximum number of related files to return
    pub max_related_files: usize,
    /// Weight for content similarity in final score
    pub content_similarity_weight: f32,
    /// Weight for session co-occurrence in final score
    pub session_weight: f32,
    /// Whether to include rejected relations in queries (for debugging)
    pub include_rejected: bool,
    /// Decay factor for relation strength over time (per day)
    pub time_decay_factor: f32,
}

impl Default for LogicChainConfig {
    fn default() -> Self {
        Self {
            min_similarity_threshold: 0.5,
            max_related_files: 10,
            content_similarity_weight: 0.6,
            session_weight: 0.4,
            include_rejected: false,
            time_decay_factor: 0.99,
        }
    }
}

/// A related file with its relation details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedFile {
    /// The relation record
    pub relation: FileRelation,
    /// The related file ID (opposite of the query file)
    pub related_file_id: Uuid,
    /// Combined score from all factors
    pub combined_score: f32,
    /// Whether this is the source or target of the relation
    pub is_source: bool,
}

/// Result of a similarity search
#[derive(Debug, Clone)]
pub struct SimilarityResult {
    /// File ID
    pub file_id: Uuid,
    /// Similarity score (0.0 - 1.0)
    pub similarity: f32,
}

/// Logic Chain Engine - handles all file relation operations
pub struct LogicChainEngine {
    pool: SqlitePool,
    config: LogicChainConfig,
    vector_store: Option<Arc<VectorStore>>,
    block_rules: Arc<RwLock<BlockRuleStore>>,
    session_tracker: Arc<RwLock<SessionTracker>>,
}

impl LogicChainEngine {
    /// Create a new LogicChainEngine
    pub async fn new(
        pool: SqlitePool,
        config: LogicChainConfig,
        vector_store: Option<Arc<VectorStore>>,
    ) -> Result<Self> {
        let block_rules = BlockRuleStore::new(pool.clone()).await?;
        let session_tracker = SessionTracker::new(pool.clone(), Default::default()).await?;

        Ok(Self {
            pool,
            config,
            vector_store,
            block_rules: Arc::new(RwLock::new(block_rules)),
            session_tracker: Arc::new(RwLock::new(session_tracker)),
        })
    }

    /// Get the session tracker
    pub fn session_tracker(&self) -> Arc<RwLock<SessionTracker>> {
        Arc::clone(&self.session_tracker)
    }

    /// Get the block rules store
    pub fn block_rules(&self) -> Arc<RwLock<BlockRuleStore>> {
        Arc::clone(&self.block_rules)
    }

    // ========================================================================
    // Relation CRUD Operations
    // ========================================================================

    /// Create a new relation between two files
    pub async fn create_relation(
        &self,
        source_file_id: Uuid,
        target_file_id: Uuid,
        relation_type: RelationType,
        strength: f32,
        source: RelationSource,
    ) -> Result<FileRelation> {
        // Validate inputs
        if source_file_id == target_file_id {
            return Err(RelationError::SelfRelation { id: source_file_id });
        }

        if !(0.0..=1.0).contains(&strength) {
            return Err(RelationError::InvalidStrength { value: strength });
        }

        // Check for existing relation
        if self.get_relation_between(source_file_id, target_file_id).await?.is_some() {
            return Err(RelationError::RelationAlreadyExists {
                source_id: source_file_id,
                target_id: target_file_id,
            });
        }

        // Check block rules for AI-generated relations
        if matches!(source, RelationSource::AIGenerated) {
            let block_rules = self.block_rules.read().await;
            if let Some(rule_id) = block_rules.is_blocked(source_file_id, target_file_id, relation_type).await? {
                return Err(RelationError::RelationBlocked { rule_id });
            }
        }

        let now = Utc::now();
        let relation = FileRelation {
            id: Uuid::now_v7(),
            source_file_id,
            target_file_id,
            relation_type,
            strength,
            source,
            user_feedback: UserFeedback::None,
            created_at: now,
            updated_at: now,
            user_action_at: None,
        };

        // Insert into database
        self.insert_relation(&relation).await?;

        info!(
            "Created relation {} between {} and {} (type: {:?}, strength: {})",
            relation.id, source_file_id, target_file_id, relation_type, strength
        );

        Ok(relation)
    }

    /// Get a relation by ID
    pub async fn get_relation(&self, id: Uuid) -> Result<Option<FileRelation>> {
        let row = sqlx::query_as::<_, RelationRow>(
            r#"
            SELECT id, source_file_id, target_file_id, relation_type, strength, source,
                   user_feedback, created_at, updated_at, user_action_at
            FROM file_relations WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| r.into_relation()).transpose()
    }

    /// Get relation between two specific files (in either direction)
    pub async fn get_relation_between(
        &self,
        file_a: Uuid,
        file_b: Uuid,
    ) -> Result<Option<FileRelation>> {
        let row = sqlx::query_as::<_, RelationRow>(
            r#"
            SELECT id, source_file_id, target_file_id, relation_type, strength, source,
                   user_feedback, created_at, updated_at, user_action_at
            FROM file_relations 
            WHERE (source_file_id = ? AND target_file_id = ?)
               OR (source_file_id = ? AND target_file_id = ?)
            "#,
        )
        .bind(file_a.to_string())
        .bind(file_b.to_string())
        .bind(file_b.to_string())
        .bind(file_a.to_string())
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| r.into_relation()).transpose()
    }

    /// Get all relations for a file (both as source and target)
    pub async fn get_relations_for_file(&self, file_id: Uuid) -> Result<Vec<RelatedFile>> {
        let file_id_str = file_id.to_string();
        
        let query = if self.config.include_rejected {
            r#"
            SELECT id, source_file_id, target_file_id, relation_type, strength, source,
                   user_feedback, created_at, updated_at, user_action_at
            FROM file_relations 
            WHERE source_file_id = ? OR target_file_id = ?
            ORDER BY strength DESC
            "#
        } else {
            r#"
            SELECT id, source_file_id, target_file_id, relation_type, strength, source,
                   user_feedback, created_at, updated_at, user_action_at
            FROM file_relations 
            WHERE (source_file_id = ? OR target_file_id = ?)
              AND user_feedback NOT LIKE '%Rejected%'
            ORDER BY strength DESC
            "#
        };

        let rows = sqlx::query_as::<_, RelationRow>(query)
            .bind(&file_id_str)
            .bind(&file_id_str)
            .fetch_all(&self.pool)
            .await?;

        let mut related_files = Vec::new();
        for row in rows {
            let relation = row.into_relation()?;
            let is_source = relation.source_file_id == file_id;
            let related_file_id = if is_source {
                relation.target_file_id
            } else {
                relation.source_file_id
            };

            // Calculate combined score with time decay
            let combined_score = self.calculate_combined_score(&relation);

            related_files.push(RelatedFile {
                relation,
                related_file_id,
                combined_score,
                is_source,
            });
        }

        // Sort by combined score and limit
        related_files.sort_by(|a, b| {
            b.combined_score
                .partial_cmp(&a.combined_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        related_files.truncate(self.config.max_related_files);

        Ok(related_files)
    }

    /// Update a relation
    pub async fn update_relation(&self, relation: &FileRelation) -> Result<()> {
        let relation_type_str = format!("{:?}", relation.relation_type);
        let source_str = format!("{:?}", relation.source);
        let user_feedback_json = serde_json::to_string(&relation.user_feedback)
            .map_err(|e| RelationError::Internal(e.to_string()))?;

        sqlx::query(
            r#"
            UPDATE file_relations 
            SET relation_type = ?, strength = ?, source = ?, user_feedback = ?,
                updated_at = ?, user_action_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&relation_type_str)
        .bind(relation.strength)
        .bind(&source_str)
        .bind(&user_feedback_json)
        .bind(relation.updated_at.to_rfc3339())
        .bind(relation.user_action_at.map(|t| t.to_rfc3339()))
        .bind(relation.id.to_string())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Delete a relation
    pub async fn delete_relation(&self, id: Uuid) -> Result<()> {
        let result = sqlx::query("DELETE FROM file_relations WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(RelationError::RelationNotFound { id });
        }

        debug!("Deleted relation {}", id);
        Ok(())
    }

    /// Delete all relations for a file
    pub async fn delete_relations_for_file(&self, file_id: Uuid) -> Result<u64> {
        let file_id_str = file_id.to_string();
        
        let result = sqlx::query(
            "DELETE FROM file_relations WHERE source_file_id = ? OR target_file_id = ?"
        )
        .bind(&file_id_str)
        .bind(&file_id_str)
        .execute(&self.pool)
        .await?;

        let deleted = result.rows_affected();
        debug!("Deleted {} relations for file {}", deleted, file_id);
        Ok(deleted)
    }

    // ========================================================================
    // Content Similarity
    // ========================================================================

    /// Find files similar to the given file based on content embeddings
    pub async fn find_similar_files(
        &self,
        file_id: Uuid,
        limit: usize,
    ) -> Result<Vec<SimilarityResult>> {
        let vector_store = match &self.vector_store {
            Some(vs) => vs,
            None => {
                warn!("Vector store not available for similarity search");
                return Ok(vec![]);
            }
        };

        // Get the file's embedding vector
        // First, find vectors associated with this file
        let filter = SearchFilter::new().with_file_ids(vec![file_id]);
        let file_vectors = vector_store
            .search(&vec![0.0; vector_store.config().vector_size as usize], 1, Some(filter))
            .await
            .map_err(|e| RelationError::VectorStore(e.to_string()))?;

        if file_vectors.is_empty() {
            debug!("No vectors found for file {}", file_id);
            return Ok(vec![]);
        }

        // Get the actual vector for this file
        let file_vector = match vector_store.get(file_vectors[0].id).await {
            Ok(Some(result)) => match result.vector {
                Some(v) => v,
                None => return Ok(vec![]),
            },
            Ok(None) => return Ok(vec![]),
            Err(e) => return Err(RelationError::VectorStore(e.to_string())),
        };

        // Search for similar vectors, excluding the source file
        let search_results = vector_store
            .search(&file_vector, limit + 1, None)
            .await
            .map_err(|e| RelationError::VectorStore(e.to_string()))?;

        // Convert to SimilarityResult, excluding the source file
        let mut results = Vec::new();
        for result in search_results {
            if let Some(result_file_id) = result.file_id() {
                if result_file_id != file_id && result.score >= self.config.min_similarity_threshold {
                    results.push(SimilarityResult {
                        file_id: result_file_id,
                        similarity: result.score,
                    });
                }
            }
        }

        results.truncate(limit);
        Ok(results)
    }

    /// Generate content-based relations for a file
    pub async fn generate_content_relations(&self, file_id: Uuid) -> Result<Vec<FileRelation>> {
        let similar_files = self.find_similar_files(file_id, self.config.max_related_files).await?;
        let mut created_relations = Vec::new();

        for similar in similar_files {
            // Check if relation already exists
            if self.get_relation_between(file_id, similar.file_id).await?.is_some() {
                continue;
            }

            // Create new relation
            match self.create_relation(
                file_id,
                similar.file_id,
                RelationType::ContentSimilar,
                similar.similarity,
                RelationSource::AIGenerated,
            ).await {
                Ok(relation) => created_relations.push(relation),
                Err(RelationError::RelationBlocked { .. }) => {
                    debug!("Relation blocked by rule: {} -> {}", file_id, similar.file_id);
                }
                Err(e) => {
                    warn!("Failed to create relation: {}", e);
                }
            }
        }

        Ok(created_relations)
    }

    // ========================================================================
    // Helper Methods
    // ========================================================================

    /// Insert a relation into the database
    async fn insert_relation(&self, relation: &FileRelation) -> Result<()> {
        let relation_type_str = format!("{:?}", relation.relation_type);
        let source_str = format!("{:?}", relation.source);
        let user_feedback_json = serde_json::to_string(&relation.user_feedback)
            .map_err(|e| RelationError::Internal(e.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO file_relations (id, source_file_id, target_file_id, relation_type, 
                                        strength, source, user_feedback, created_at, 
                                        updated_at, user_action_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(relation.id.to_string())
        .bind(relation.source_file_id.to_string())
        .bind(relation.target_file_id.to_string())
        .bind(&relation_type_str)
        .bind(relation.strength)
        .bind(&source_str)
        .bind(&user_feedback_json)
        .bind(relation.created_at.to_rfc3339())
        .bind(relation.updated_at.to_rfc3339())
        .bind(relation.user_action_at.map(|t| t.to_rfc3339()))
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Calculate combined score with time decay
    fn calculate_combined_score(&self, relation: &FileRelation) -> f32 {
        let base_score = relation.strength;
        
        // Apply user feedback adjustments
        let feedback_adjusted = match &relation.user_feedback {
            UserFeedback::None => base_score,
            UserFeedback::Confirmed => (base_score * 1.2).min(1.0), // Boost confirmed
            UserFeedback::Rejected { .. } => 0.0, // Zero out rejected
            UserFeedback::Adjusted { user_strength, .. } => *user_strength,
        };

        // Apply time decay
        let days_old = (Utc::now() - relation.updated_at).num_days() as f32;
        let time_factor = self.config.time_decay_factor.powf(days_old.max(0.0));

        feedback_adjusted * time_factor
    }
}

// ============================================================================
// Database Row Types
// ============================================================================

#[derive(sqlx::FromRow)]
struct RelationRow {
    id: String,
    source_file_id: String,
    target_file_id: String,
    relation_type: String,
    strength: f64,
    source: String,
    user_feedback: String,
    created_at: String,
    updated_at: String,
    user_action_at: Option<String>,
}

impl RelationRow {
    fn into_relation(self) -> Result<FileRelation> {
        let relation_type = match self.relation_type.as_str() {
            "ContentSimilar" => RelationType::ContentSimilar,
            "SameSession" => RelationType::SameSession,
            "SameProject" => RelationType::SameProject,
            "SameAuthor" => RelationType::SameAuthor,
            "Reference" => RelationType::Reference,
            "Derivative" => RelationType::Derivative,
            "Workflow" => RelationType::Workflow,
            "UserDefined" => RelationType::UserDefined,
            _ => RelationType::ContentSimilar,
        };

        let source = match self.source.as_str() {
            "AIGenerated" => RelationSource::AIGenerated,
            "SessionTracking" => RelationSource::SessionTracking,
            "UserManual" => RelationSource::UserManual,
            "MetadataExtract" => RelationSource::MetadataExtract,
            _ => RelationSource::AIGenerated,
        };

        let user_feedback: UserFeedback = serde_json::from_str(&self.user_feedback)
            .unwrap_or(UserFeedback::None);

        Ok(FileRelation {
            id: Uuid::parse_str(&self.id).map_err(|e| RelationError::Internal(e.to_string()))?,
            source_file_id: Uuid::parse_str(&self.source_file_id)
                .map_err(|e| RelationError::Internal(e.to_string()))?,
            target_file_id: Uuid::parse_str(&self.target_file_id)
                .map_err(|e| RelationError::Internal(e.to_string()))?,
            relation_type,
            strength: self.strength as f32,
            source,
            user_feedback,
            created_at: DateTime::parse_from_rfc3339(&self.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| RelationError::Internal(e.to_string()))?,
            updated_at: DateTime::parse_from_rfc3339(&self.updated_at)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| RelationError::Internal(e.to_string()))?,
            user_action_at: self.user_action_at
                .as_deref()
                .map(|s| DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&Utc)))
                .transpose()
                .map_err(|e| RelationError::Internal(e.to_string()))?,
        })
    }
}
