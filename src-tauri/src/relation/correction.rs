//! Relation Correction API - Human-in-the-loop relation management
//!
//! Provides APIs for users to confirm, reject, and adjust file relations.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tracing::{debug, info};
use uuid::Uuid;

use crate::core::types::{FileRelation, RelationType, RelationSource, UserFeedback, RelationBlockRule, BlockRuleType, BlockRuleDetail};
use super::error::{RelationError, Result};
use super::engine::LogicChainEngine;
use super::block_rules::BlockRuleStore;

/// Relation correction command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RelationCommand {
    /// Confirm relation is valid
    Confirm {
        relation_id: Uuid,
    },

    /// Reject relation (one-click unlink)
    Reject {
        relation_id: Uuid,
        reason: Option<String>,
        /// Whether to block similar relations
        block_similar: bool,
        /// Block scope
        block_scope: Option<BlockScope>,
    },

    /// Adjust relation strength
    Adjust {
        relation_id: Uuid,
        new_strength: f32,
    },

    /// Manually create relation
    Create {
        source_file_id: Uuid,
        target_file_id: Uuid,
        relation_type: RelationType,
        strength: f32,
    },

    /// Batch reject (e.g., reject all AI relations for a file)
    BatchReject {
        file_id: Uuid,
        relation_types: Option<Vec<RelationType>>,
        block_future: bool,
    },
}

/// Block scope for rejected relations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BlockScope {
    /// Only block this specific pair
    ThisPairOnly,

    /// Block source file with all files under target's tag
    SourceToTargetTag {
        target_tag_id: Uuid,
    },

    /// Block all files under source tag with all files under target tag
    TagToTag {
        source_tag_id: Uuid,
        target_tag_id: Uuid,
    },
}

/// Result of a relation correction operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationCorrectionResult {
    /// Whether the operation succeeded
    pub success: bool,
    /// IDs of affected relations
    pub affected_relations: Vec<Uuid>,
    /// IDs of created block rules
    pub created_block_rules: Vec<Uuid>,
    /// Human-readable message
    pub message: String,
}

/// Relation Correction Service - handles human-in-the-loop corrections
pub struct RelationCorrectionService {
    pool: SqlitePool,
}

impl RelationCorrectionService {
    /// Create a new RelationCorrectionService
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Execute a relation correction command
    pub async fn execute(
        &self,
        cmd: RelationCommand,
        engine: &LogicChainEngine,
    ) -> Result<RelationCorrectionResult> {
        match cmd {
            RelationCommand::Confirm { relation_id } => {
                self.confirm_relation(relation_id, engine).await
            }
            RelationCommand::Reject {
                relation_id,
                reason,
                block_similar,
                block_scope,
            } => {
                self.reject_relation(relation_id, reason, block_similar, block_scope, engine).await
            }
            RelationCommand::Adjust {
                relation_id,
                new_strength,
            } => {
                self.adjust_relation(relation_id, new_strength, engine).await
            }
            RelationCommand::Create {
                source_file_id,
                target_file_id,
                relation_type,
                strength,
            } => {
                self.create_manual_relation(source_file_id, target_file_id, relation_type, strength, engine).await
            }
            RelationCommand::BatchReject {
                file_id,
                relation_types,
                block_future,
            } => {
                self.batch_reject(file_id, relation_types, block_future, engine).await
            }
        }
    }

    /// Confirm a relation as valid
    async fn confirm_relation(
        &self,
        relation_id: Uuid,
        engine: &LogicChainEngine,
    ) -> Result<RelationCorrectionResult> {
        let mut relation = engine
            .get_relation(relation_id)
            .await?
            .ok_or(RelationError::RelationNotFound { id: relation_id })?;

        // Validate state transition
        self.validate_feedback_transition(&relation.user_feedback, &UserFeedback::Confirmed)?;

        // Update feedback
        relation.user_feedback = UserFeedback::Confirmed;
        relation.updated_at = Utc::now();
        relation.user_action_at = Some(Utc::now());

        engine.update_relation(&relation).await?;

        info!("Confirmed relation {}", relation_id);

        Ok(RelationCorrectionResult {
            success: true,
            affected_relations: vec![relation_id],
            created_block_rules: vec![],
            message: "Relation confirmed successfully".to_string(),
        })
    }

    /// Reject a relation
    async fn reject_relation(
        &self,
        relation_id: Uuid,
        reason: Option<String>,
        block_similar: bool,
        block_scope: Option<BlockScope>,
        engine: &LogicChainEngine,
    ) -> Result<RelationCorrectionResult> {
        let mut relation = engine
            .get_relation(relation_id)
            .await?
            .ok_or(RelationError::RelationNotFound { id: relation_id })?;

        let new_feedback = UserFeedback::Rejected {
            reason: reason.clone(),
            block_similar,
        };

        // Validate state transition
        self.validate_feedback_transition(&relation.user_feedback, &new_feedback)?;

        // Update feedback
        relation.user_feedback = new_feedback;
        relation.updated_at = Utc::now();
        relation.user_action_at = Some(Utc::now());

        engine.update_relation(&relation).await?;

        // Create block rules if requested
        let mut created_block_rules = Vec::new();
        if block_similar {
            let block_rules = engine.block_rules();
            let mut rules = block_rules.write().await;

            match block_scope {
                Some(BlockScope::ThisPairOnly) | None => {
                    let rule = rules
                        .create_file_pair_rule(
                            relation.source_file_id,
                            relation.target_file_id,
                        )
                        .await?;
                    created_block_rules.push(rule.id);
                }
                Some(BlockScope::SourceToTargetTag { target_tag_id }) => {
                    let rule = rules
                        .create_file_to_tag_rule(relation.source_file_id, target_tag_id)
                        .await?;
                    created_block_rules.push(rule.id);
                }
                Some(BlockScope::TagToTag {
                    source_tag_id,
                    target_tag_id,
                }) => {
                    let rule = rules
                        .create_tag_pair_rule(source_tag_id, target_tag_id)
                        .await?;
                    created_block_rules.push(rule.id);
                }
            }
        }

        info!(
            "Rejected relation {} (block_similar: {}, rules created: {})",
            relation_id,
            block_similar,
            created_block_rules.len()
        );

        Ok(RelationCorrectionResult {
            success: true,
            affected_relations: vec![relation_id],
            created_block_rules,
            message: format!(
                "Relation rejected{}",
                if block_similar {
                    " and similar relations blocked"
                } else {
                    ""
                }
            ),
        })
    }

    /// Adjust relation strength
    async fn adjust_relation(
        &self,
        relation_id: Uuid,
        new_strength: f32,
        engine: &LogicChainEngine,
    ) -> Result<RelationCorrectionResult> {
        if !(0.0..=1.0).contains(&new_strength) {
            return Err(RelationError::InvalidStrength { value: new_strength });
        }

        let mut relation = engine
            .get_relation(relation_id)
            .await?
            .ok_or(RelationError::RelationNotFound { id: relation_id })?;

        let original_strength = relation.strength;
        let new_feedback = UserFeedback::Adjusted {
            original_strength,
            user_strength: new_strength,
        };

        // Validate state transition
        self.validate_feedback_transition(&relation.user_feedback, &new_feedback)?;

        // Update relation
        relation.user_feedback = new_feedback;
        relation.strength = new_strength;
        relation.updated_at = Utc::now();
        relation.user_action_at = Some(Utc::now());

        engine.update_relation(&relation).await?;

        info!(
            "Adjusted relation {} strength from {} to {}",
            relation_id, original_strength, new_strength
        );

        Ok(RelationCorrectionResult {
            success: true,
            affected_relations: vec![relation_id],
            created_block_rules: vec![],
            message: format!(
                "Relation strength adjusted from {:.2} to {:.2}",
                original_strength, new_strength
            ),
        })
    }

    /// Create a manual relation
    async fn create_manual_relation(
        &self,
        source_file_id: Uuid,
        target_file_id: Uuid,
        relation_type: RelationType,
        strength: f32,
        engine: &LogicChainEngine,
    ) -> Result<RelationCorrectionResult> {
        let relation = engine
            .create_relation(
                source_file_id,
                target_file_id,
                relation_type,
                strength,
                RelationSource::UserManual,
            )
            .await?;

        info!(
            "Created manual relation {} between {} and {}",
            relation.id, source_file_id, target_file_id
        );

        Ok(RelationCorrectionResult {
            success: true,
            affected_relations: vec![relation.id],
            created_block_rules: vec![],
            message: "Manual relation created successfully".to_string(),
        })
    }

    /// Batch reject relations for a file
    async fn batch_reject(
        &self,
        file_id: Uuid,
        relation_types: Option<Vec<RelationType>>,
        block_future: bool,
        engine: &LogicChainEngine,
    ) -> Result<RelationCorrectionResult> {
        // Get all relations for the file
        let relations = engine.get_relations_for_file(file_id).await?;

        let mut affected_relations = Vec::new();
        let now = Utc::now();

        for related in relations {
            // Filter by relation type if specified
            if let Some(ref types) = relation_types {
                if !types.contains(&related.relation.relation_type) {
                    continue;
                }
            }

            // Only reject AI-generated relations
            if !matches!(related.relation.source, RelationSource::AIGenerated) {
                continue;
            }

            // Update the relation
            let mut relation = related.relation;
            relation.user_feedback = UserFeedback::Rejected {
                reason: Some("Batch rejection".to_string()),
                block_similar: block_future,
            };
            relation.updated_at = now;
            relation.user_action_at = Some(now);

            engine.update_relation(&relation).await?;
            affected_relations.push(relation.id);
        }

        // Create block rule if requested
        let mut created_block_rules = Vec::new();
        if block_future {
            let block_rules = engine.block_rules();
            let mut rules = block_rules.write().await;
            let rule = rules.create_file_all_ai_rule(file_id).await?;
            created_block_rules.push(rule.id);
        }

        info!(
            "Batch rejected {} relations for file {} (block_future: {})",
            affected_relations.len(),
            file_id,
            block_future
        );

        Ok(RelationCorrectionResult {
            success: true,
            affected_relations,
            created_block_rules,
            message: format!(
                "Rejected {} AI-generated relations{}",
                affected_relations.len(),
                if block_future {
                    " and blocked future AI relations"
                } else {
                    ""
                }
            ),
        })
    }

    /// Validate user feedback state transition
    fn validate_feedback_transition(
        &self,
        from: &UserFeedback,
        to: &UserFeedback,
    ) -> Result<()> {
        // Define valid transitions
        let valid = match (from, to) {
            // From None, can go to any state
            (UserFeedback::None, _) => true,
            // From Confirmed, can go to Rejected or Adjusted
            (UserFeedback::Confirmed, UserFeedback::Rejected { .. }) => true,
            (UserFeedback::Confirmed, UserFeedback::Adjusted { .. }) => true,
            // From Adjusted, can go to Confirmed or Rejected
            (UserFeedback::Adjusted { .. }, UserFeedback::Confirmed) => true,
            (UserFeedback::Adjusted { .. }, UserFeedback::Rejected { .. }) => true,
            // Can re-adjust
            (UserFeedback::Adjusted { .. }, UserFeedback::Adjusted { .. }) => true,
            // From Rejected, can only go to Confirmed (un-reject)
            (UserFeedback::Rejected { .. }, UserFeedback::Confirmed) => true,
            // Same state transitions are allowed (idempotent)
            (UserFeedback::Confirmed, UserFeedback::Confirmed) => true,
            // All other transitions are invalid
            _ => false,
        };

        if !valid {
            return Err(RelationError::InvalidFeedbackTransition {
                from: format!("{:?}", from),
                to: format!("{:?}", to),
            });
        }

        Ok(())
    }

    /// Get all relations for a file (including user feedback status)
    pub async fn get_relations(
        &self,
        file_id: Uuid,
        engine: &LogicChainEngine,
    ) -> Result<Vec<FileRelation>> {
        let related = engine.get_relations_for_file(file_id).await?;
        Ok(related.into_iter().map(|r| r.relation).collect())
    }

    /// Get block rules for a file (or all if file_id is None)
    pub async fn get_block_rules(
        &self,
        file_id: Option<Uuid>,
        engine: &LogicChainEngine,
    ) -> Result<Vec<RelationBlockRule>> {
        let block_rules = engine.block_rules();
        let rules = block_rules.read().await;
        rules.get_rules_for_file(file_id).await
    }

    /// Remove a block rule
    pub async fn remove_block_rule(
        &self,
        rule_id: Uuid,
        engine: &LogicChainEngine,
    ) -> Result<()> {
        let block_rules = engine.block_rules();
        let mut rules = block_rules.write().await;
        rules.delete_rule(rule_id).await
    }
}
