//! Block Rules Store - Manages relation block rules
//!
//! Prevents AI from regenerating rejected relations based on user-defined rules.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tracing::{debug, info};
use uuid::Uuid;

use crate::core::types::{RelationBlockRule, BlockRuleType, BlockRuleDetail, RelationType};
use super::error::{RelationError, Result};

/// Filter for querying block rules
#[derive(Debug, Clone, Default)]
pub struct BlockRuleFilter {
    /// Filter by rule type
    pub rule_type: Option<BlockRuleType>,
    /// Filter by file ID (matches any file in the rule)
    pub file_id: Option<Uuid>,
    /// Filter by tag ID (matches any tag in the rule)
    pub tag_id: Option<Uuid>,
    /// Only include active rules
    pub active_only: bool,
    /// Only include non-expired rules
    pub exclude_expired: bool,
}

impl BlockRuleFilter {
    /// Create a new filter
    pub fn new() -> Self {
        Self {
            active_only: true,
            exclude_expired: true,
            ..Default::default()
        }
    }

    /// Filter by rule type
    pub fn with_rule_type(mut self, rule_type: BlockRuleType) -> Self {
        self.rule_type = Some(rule_type);
        self
    }

    /// Filter by file ID
    pub fn with_file_id(mut self, file_id: Uuid) -> Self {
        self.file_id = Some(file_id);
        self
    }

    /// Filter by tag ID
    pub fn with_tag_id(mut self, tag_id: Uuid) -> Self {
        self.tag_id = Some(tag_id);
        self
    }

    /// Include inactive rules
    pub fn include_inactive(mut self) -> Self {
        self.active_only = false;
        self
    }

    /// Include expired rules
    pub fn include_expired(mut self) -> Self {
        self.exclude_expired = false;
        self
    }
}

/// Block Rule Store - manages relation block rules
pub struct BlockRuleStore {
    pool: SqlitePool,
}

impl BlockRuleStore {
    /// Create a new BlockRuleStore
    pub async fn new(pool: SqlitePool) -> Result<Self> {
        Ok(Self { pool })
    }

    // ========================================================================
    // Rule Creation
    // ========================================================================

    /// Create a file pair block rule
    pub async fn create_file_pair_rule(
        &mut self,
        file_id_a: Uuid,
        file_id_b: Uuid,
    ) -> Result<RelationBlockRule> {
        let rule = RelationBlockRule {
            id: Uuid::now_v7(),
            rule_type: BlockRuleType::FilePair,
            rule_detail: BlockRuleDetail::FilePair { file_id_a, file_id_b },
            created_at: Utc::now(),
            expires_at: None,
            is_active: true,
        };

        self.insert_rule(&rule).await?;
        info!("Created file pair block rule: {} <-> {}", file_id_a, file_id_b);
        Ok(rule)
    }

    /// Create a file-to-tag block rule
    pub async fn create_file_to_tag_rule(
        &mut self,
        file_id: Uuid,
        tag_id: Uuid,
    ) -> Result<RelationBlockRule> {
        let rule = RelationBlockRule {
            id: Uuid::now_v7(),
            rule_type: BlockRuleType::FileToTag,
            rule_detail: BlockRuleDetail::FileToTag { file_id, tag_id },
            created_at: Utc::now(),
            expires_at: None,
            is_active: true,
        };

        self.insert_rule(&rule).await?;
        info!("Created file-to-tag block rule: {} -> tag {}", file_id, tag_id);
        Ok(rule)
    }

    /// Create a tag pair block rule
    pub async fn create_tag_pair_rule(
        &mut self,
        tag_id_a: Uuid,
        tag_id_b: Uuid,
    ) -> Result<RelationBlockRule> {
        let rule = RelationBlockRule {
            id: Uuid::now_v7(),
            rule_type: BlockRuleType::TagPair,
            rule_detail: BlockRuleDetail::TagPair { tag_id_a, tag_id_b },
            created_at: Utc::now(),
            expires_at: None,
            is_active: true,
        };

        self.insert_rule(&rule).await?;
        info!("Created tag pair block rule: tag {} <-> tag {}", tag_id_a, tag_id_b);
        Ok(rule)
    }

    /// Create a file-all-AI block rule
    pub async fn create_file_all_ai_rule(&mut self, file_id: Uuid) -> Result<RelationBlockRule> {
        let rule = RelationBlockRule {
            id: Uuid::now_v7(),
            rule_type: BlockRuleType::FileAllAI,
            rule_detail: BlockRuleDetail::FileAllAI { file_id },
            created_at: Utc::now(),
            expires_at: None,
            is_active: true,
        };

        self.insert_rule(&rule).await?;
        info!("Created file-all-AI block rule for file {}", file_id);
        Ok(rule)
    }

    /// Create a relation type block rule
    pub async fn create_relation_type_rule(
        &mut self,
        file_id: Option<Uuid>,
        relation_type: RelationType,
    ) -> Result<RelationBlockRule> {
        let rule = RelationBlockRule {
            id: Uuid::now_v7(),
            rule_type: BlockRuleType::RelationType,
            rule_detail: BlockRuleDetail::RelationType { file_id, relation_type },
            created_at: Utc::now(),
            expires_at: None,
            is_active: true,
        };

        self.insert_rule(&rule).await?;
        info!(
            "Created relation type block rule: {:?} for file {:?}",
            relation_type, file_id
        );
        Ok(rule)
    }

    // ========================================================================
    // Rule Queries
    // ========================================================================

    /// Get a rule by ID
    pub async fn get_rule(&self, id: Uuid) -> Result<Option<RelationBlockRule>> {
        let row = sqlx::query_as::<_, BlockRuleRow>(
            r#"
            SELECT id, rule_type, rule_detail, created_at, expires_at, is_active
            FROM relation_block_rules WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| r.into_rule()).transpose()
    }

    /// Get all rules matching a filter
    pub async fn get_rules(&self, filter: BlockRuleFilter) -> Result<Vec<RelationBlockRule>> {
        let mut query = String::from(
            "SELECT id, rule_type, rule_detail, created_at, expires_at, is_active FROM relation_block_rules WHERE 1=1"
        );

        if filter.active_only {
            query.push_str(" AND is_active = 1");
        }

        if filter.exclude_expired {
            query.push_str(" AND (expires_at IS NULL OR expires_at > datetime('now'))");
        }

        if let Some(rule_type) = filter.rule_type {
            query.push_str(&format!(" AND rule_type = '{:?}'", rule_type));
        }

        query.push_str(" ORDER BY created_at DESC");

        let rows = sqlx::query_as::<_, BlockRuleRow>(&query)
            .fetch_all(&self.pool)
            .await?;

        let mut rules = Vec::new();
        for row in rows {
            let rule = row.into_rule()?;
            
            // Apply file_id and tag_id filters in memory (complex JSON filtering)
            if let Some(file_id) = filter.file_id {
                if !self.rule_matches_file(&rule, file_id) {
                    continue;
                }
            }
            
            if let Some(tag_id) = filter.tag_id {
                if !self.rule_matches_tag(&rule, tag_id) {
                    continue;
                }
            }
            
            rules.push(rule);
        }

        Ok(rules)
    }

    /// Get rules for a specific file
    pub async fn get_rules_for_file(&self, file_id: Option<Uuid>) -> Result<Vec<RelationBlockRule>> {
        let filter = match file_id {
            Some(id) => BlockRuleFilter::new().with_file_id(id),
            None => BlockRuleFilter::new(),
        };
        self.get_rules(filter).await
    }

    /// Check if a relation is blocked by any rule
    /// Returns the blocking rule ID if blocked, None otherwise
    pub async fn is_blocked(
        &self,
        source_file_id: Uuid,
        target_file_id: Uuid,
        relation_type: RelationType,
    ) -> Result<Option<Uuid>> {
        let rules = self.get_rules(BlockRuleFilter::new()).await?;

        for rule in rules {
            if self.rule_blocks_relation(&rule, source_file_id, target_file_id, relation_type) {
                debug!(
                    "Relation {} -> {} blocked by rule {}",
                    source_file_id, target_file_id, rule.id
                );
                return Ok(Some(rule.id));
            }
        }

        Ok(None)
    }

    // ========================================================================
    // Rule Management
    // ========================================================================

    /// Delete a rule
    pub async fn delete_rule(&mut self, id: Uuid) -> Result<()> {
        let result = sqlx::query("DELETE FROM relation_block_rules WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(RelationError::BlockRuleNotFound { id });
        }

        info!("Deleted block rule {}", id);
        Ok(())
    }

    /// Deactivate a rule (soft delete)
    pub async fn deactivate_rule(&mut self, id: Uuid) -> Result<()> {
        let result = sqlx::query("UPDATE relation_block_rules SET is_active = 0 WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(RelationError::BlockRuleNotFound { id });
        }

        info!("Deactivated block rule {}", id);
        Ok(())
    }

    /// Reactivate a rule
    pub async fn reactivate_rule(&mut self, id: Uuid) -> Result<()> {
        let result = sqlx::query("UPDATE relation_block_rules SET is_active = 1 WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(RelationError::BlockRuleNotFound { id });
        }

        info!("Reactivated block rule {}", id);
        Ok(())
    }

    /// Set expiration for a rule
    pub async fn set_expiration(&mut self, id: Uuid, expires_at: Option<DateTime<Utc>>) -> Result<()> {
        let result = sqlx::query("UPDATE relation_block_rules SET expires_at = ? WHERE id = ?")
            .bind(expires_at.map(|t| t.to_rfc3339()))
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(RelationError::BlockRuleNotFound { id });
        }

        info!("Set expiration for block rule {}: {:?}", id, expires_at);
        Ok(())
    }

    /// Clean up expired rules
    pub async fn cleanup_expired(&mut self) -> Result<u64> {
        let result = sqlx::query(
            "DELETE FROM relation_block_rules WHERE expires_at IS NOT NULL AND expires_at <= datetime('now')"
        )
        .execute(&self.pool)
        .await?;

        let deleted = result.rows_affected();
        if deleted > 0 {
            info!("Cleaned up {} expired block rules", deleted);
        }

        Ok(deleted)
    }

    // ========================================================================
    // Helper Methods
    // ========================================================================

    /// Insert a rule into the database
    async fn insert_rule(&self, rule: &RelationBlockRule) -> Result<()> {
        let rule_type_str = format!("{:?}", rule.rule_type);
        let rule_detail_json = serde_json::to_string(&rule.rule_detail)
            .map_err(|e| RelationError::Internal(e.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO relation_block_rules (id, rule_type, rule_detail, created_at, expires_at, is_active)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(rule.id.to_string())
        .bind(&rule_type_str)
        .bind(&rule_detail_json)
        .bind(rule.created_at.to_rfc3339())
        .bind(rule.expires_at.map(|t| t.to_rfc3339()))
        .bind(rule.is_active)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Check if a rule matches a file ID
    fn rule_matches_file(&self, rule: &RelationBlockRule, file_id: Uuid) -> bool {
        match &rule.rule_detail {
            BlockRuleDetail::FilePair { file_id_a, file_id_b } => {
                *file_id_a == file_id || *file_id_b == file_id
            }
            BlockRuleDetail::FileToTag { file_id: fid, .. } => *fid == file_id,
            BlockRuleDetail::FileAllAI { file_id: fid } => *fid == file_id,
            BlockRuleDetail::RelationType { file_id: Some(fid), .. } => *fid == file_id,
            _ => false,
        }
    }

    /// Check if a rule matches a tag ID
    fn rule_matches_tag(&self, rule: &RelationBlockRule, tag_id: Uuid) -> bool {
        match &rule.rule_detail {
            BlockRuleDetail::FileToTag { tag_id: tid, .. } => *tid == tag_id,
            BlockRuleDetail::TagPair { tag_id_a, tag_id_b } => {
                *tag_id_a == tag_id || *tag_id_b == tag_id
            }
            _ => false,
        }
    }

    /// Check if a rule blocks a specific relation
    fn rule_blocks_relation(
        &self,
        rule: &RelationBlockRule,
        source_file_id: Uuid,
        target_file_id: Uuid,
        relation_type: RelationType,
    ) -> bool {
        match &rule.rule_detail {
            BlockRuleDetail::FilePair { file_id_a, file_id_b } => {
                // Block if either direction matches
                (*file_id_a == source_file_id && *file_id_b == target_file_id)
                    || (*file_id_a == target_file_id && *file_id_b == source_file_id)
            }
            BlockRuleDetail::FileToTag { file_id, .. } => {
                // Block if source file matches (target tag check would need tag lookup)
                *file_id == source_file_id
            }
            BlockRuleDetail::TagPair { .. } => {
                // Would need tag lookup for both files - simplified for now
                false
            }
            BlockRuleDetail::FileAllAI { file_id } => {
                // Block all AI relations for this file
                *file_id == source_file_id || *file_id == target_file_id
            }
            BlockRuleDetail::RelationType {
                file_id,
                relation_type: blocked_type,
            } => {
                // Block specific relation type
                if *blocked_type != relation_type {
                    return false;
                }
                match file_id {
                    Some(fid) => *fid == source_file_id || *fid == target_file_id,
                    None => true, // Global block for this relation type
                }
            }
        }
    }
}

// ============================================================================
// Database Row Types
// ============================================================================

#[derive(sqlx::FromRow)]
struct BlockRuleRow {
    id: String,
    rule_type: String,
    rule_detail: String,
    created_at: String,
    expires_at: Option<String>,
    is_active: bool,
}

impl BlockRuleRow {
    fn into_rule(self) -> Result<RelationBlockRule> {
        let rule_type = match self.rule_type.as_str() {
            "FilePair" => BlockRuleType::FilePair,
            "FileToTag" => BlockRuleType::FileToTag,
            "TagPair" => BlockRuleType::TagPair,
            "FileAllAI" => BlockRuleType::FileAllAI,
            "RelationType" => BlockRuleType::RelationType,
            _ => BlockRuleType::FilePair,
        };

        let rule_detail: BlockRuleDetail = serde_json::from_str(&self.rule_detail)
            .map_err(|e| RelationError::Internal(e.to_string()))?;

        Ok(RelationBlockRule {
            id: Uuid::parse_str(&self.id).map_err(|e| RelationError::Internal(e.to_string()))?,
            rule_type,
            rule_detail,
            created_at: DateTime::parse_from_rfc3339(&self.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| RelationError::Internal(e.to_string()))?,
            expires_at: self.expires_at
                .as_deref()
                .map(|s| DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&Utc)))
                .transpose()
                .map_err(|e| RelationError::Internal(e.to_string()))?,
            is_active: self.is_active,
        })
    }
}
