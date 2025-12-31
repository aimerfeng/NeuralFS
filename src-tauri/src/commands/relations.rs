//! Relation Commands for NeuralFS
//!
//! Provides Tauri commands for file relation management:
//! - get_relations: Get related files for a given file
//! - confirm_relation: Confirm a relation as valid
//! - reject_relation: Reject a relation (one-click unlink)
//! - block_relation: Block similar relations from being generated
//!
//! **Validates: Requirements 6.1, Human-in-the-Loop**

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::types::{FileRelation, RelationType, RelationSource, UserFeedback};
use crate::relation::{RelationCommand, BlockScope};

/// Related file DTO for frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedFileDto {
    /// Relation ID
    pub relation_id: String,
    /// Related file ID
    pub file_id: String,
    /// File path
    pub path: String,
    /// File name
    pub filename: String,
    /// Relation type
    pub relation_type: String,
    /// Relation strength (0.0 - 1.0)
    pub strength: f32,
    /// Relation source
    pub source: String,
    /// User feedback status
    pub feedback_status: String,
    /// Whether this is a bidirectional relation
    pub is_bidirectional: bool,
}

/// Request to confirm a relation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmRelationRequest {
    /// Relation ID
    pub relation_id: String,
}

/// Request to reject a relation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectRelationRequest {
    /// Relation ID
    pub relation_id: String,
    /// Rejection reason (optional)
    pub reason: Option<String>,
    /// Whether to block similar relations
    pub block_similar: Option<bool>,
}

/// Request to block a relation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockRelationRequest {
    /// Relation ID
    pub relation_id: String,
    /// Block scope
    pub scope: String,
    /// Target tag ID (for tag-based blocking)
    pub target_tag_id: Option<String>,
    /// Source tag ID (for tag-to-tag blocking)
    pub source_tag_id: Option<String>,
}

/// Request to create a manual relation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRelationRequest {
    /// Source file ID
    pub source_file_id: String,
    /// Target file ID
    pub target_file_id: String,
    /// Relation type
    pub relation_type: String,
    /// Relation strength (0.0 - 1.0)
    pub strength: Option<f32>,
}

/// Relation operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationOperationResult {
    /// Whether the operation succeeded
    pub success: bool,
    /// Human-readable message
    pub message: String,
    /// Affected relation IDs
    pub affected_relations: Vec<String>,
    /// Created block rule IDs
    pub created_block_rules: Vec<String>,
}

/// Get related files for a given file
///
/// Returns all files related to the specified file, including:
/// - Content similarity relations
/// - Session-based relations
/// - User-defined relations
///
/// # Arguments
/// * `file_id` - File ID to get relations for
/// * `include_rejected` - Whether to include rejected relations
///
/// # Returns
/// List of related files
#[tauri::command]
pub async fn get_relations(
    file_id: String,
    include_rejected: Option<bool>,
) -> Result<Vec<RelatedFileDto>, String> {
    let _file_uuid = Uuid::parse_str(&file_id)
        .map_err(|e| format!("Invalid file_id: {}", e))?;

    let _include_rejected = include_rejected.unwrap_or(false);

    // In production, this would query the LogicChainEngine
    // For now, return empty list
    Ok(vec![])
}

/// Confirm a relation as valid
///
/// Marks a relation as confirmed by the user.
/// This feedback is used to improve future relation suggestions.
///
/// # Arguments
/// * `request` - Confirm relation request
///
/// # Returns
/// Operation result
#[tauri::command]
pub async fn confirm_relation(request: ConfirmRelationRequest) -> Result<RelationOperationResult, String> {
    let relation_uuid = Uuid::parse_str(&request.relation_id)
        .map_err(|e| format!("Invalid relation_id: {}", e))?;

    // In production, this would use RelationCorrectionService
    Ok(RelationOperationResult {
        success: true,
        message: "Relation confirmed successfully".to_string(),
        affected_relations: vec![relation_uuid.to_string()],
        created_block_rules: vec![],
    })
}

/// Reject a relation (one-click unlink)
///
/// Marks a relation as rejected by the user.
/// Optionally blocks similar relations from being generated.
///
/// # Arguments
/// * `request` - Reject relation request
///
/// # Returns
/// Operation result
#[tauri::command]
pub async fn reject_relation(request: RejectRelationRequest) -> Result<RelationOperationResult, String> {
    let relation_uuid = Uuid::parse_str(&request.relation_id)
        .map_err(|e| format!("Invalid relation_id: {}", e))?;

    let block_similar = request.block_similar.unwrap_or(false);

    // In production, this would use RelationCorrectionService
    let message = if block_similar {
        "Relation rejected and similar relations blocked"
    } else {
        "Relation rejected"
    };

    Ok(RelationOperationResult {
        success: true,
        message: message.to_string(),
        affected_relations: vec![relation_uuid.to_string()],
        created_block_rules: if block_similar {
            vec![Uuid::now_v7().to_string()]
        } else {
            vec![]
        },
    })
}

/// Block similar relations
///
/// Creates a block rule to prevent similar relations from being generated.
///
/// # Arguments
/// * `request` - Block relation request
///
/// # Returns
/// Operation result
#[tauri::command]
pub async fn block_relation(request: BlockRelationRequest) -> Result<RelationOperationResult, String> {
    let relation_uuid = Uuid::parse_str(&request.relation_id)
        .map_err(|e| format!("Invalid relation_id: {}", e))?;

    // Parse block scope
    let scope = parse_block_scope(&request)?;

    // In production, this would use RelationCorrectionService
    let block_rule_id = Uuid::now_v7();

    Ok(RelationOperationResult {
        success: true,
        message: format!("Block rule created with scope: {}", request.scope),
        affected_relations: vec![relation_uuid.to_string()],
        created_block_rules: vec![block_rule_id.to_string()],
    })
}

/// Create a manual relation between two files
///
/// Creates a user-defined relation between two files.
///
/// # Arguments
/// * `request` - Create relation request
///
/// # Returns
/// Operation result
#[tauri::command]
pub async fn create_relation(request: CreateRelationRequest) -> Result<RelationOperationResult, String> {
    let source_uuid = Uuid::parse_str(&request.source_file_id)
        .map_err(|e| format!("Invalid source_file_id: {}", e))?;
    let target_uuid = Uuid::parse_str(&request.target_file_id)
        .map_err(|e| format!("Invalid target_file_id: {}", e))?;

    let relation_type = parse_relation_type(&request.relation_type)?;
    let strength = request.strength.unwrap_or(0.8);

    if !(0.0..=1.0).contains(&strength) {
        return Err("Strength must be between 0.0 and 1.0".to_string());
    }

    // In production, this would use LogicChainEngine
    let new_relation_id = Uuid::now_v7();

    Ok(RelationOperationResult {
        success: true,
        message: "Relation created successfully".to_string(),
        affected_relations: vec![new_relation_id.to_string()],
        created_block_rules: vec![],
    })
}

/// Get block rules for a file
///
/// Returns all block rules that apply to a specific file.
///
/// # Arguments
/// * `file_id` - Optional file ID (if None, returns all rules)
///
/// # Returns
/// List of block rules
#[tauri::command]
pub async fn get_block_rules(file_id: Option<String>) -> Result<Vec<BlockRuleDto>, String> {
    let _file_uuid = file_id
        .as_ref()
        .map(|id| Uuid::parse_str(id))
        .transpose()
        .map_err(|e| format!("Invalid file_id: {}", e))?;

    // In production, this would query BlockRuleStore
    Ok(vec![])
}

/// Remove a block rule
///
/// Deletes a block rule, allowing similar relations to be generated again.
///
/// # Arguments
/// * `rule_id` - Block rule ID
///
/// # Returns
/// Operation result
#[tauri::command]
pub async fn remove_block_rule(rule_id: String) -> Result<RelationOperationResult, String> {
    let rule_uuid = Uuid::parse_str(&rule_id)
        .map_err(|e| format!("Invalid rule_id: {}", e))?;

    // In production, this would use RelationCorrectionService
    Ok(RelationOperationResult {
        success: true,
        message: "Block rule removed".to_string(),
        affected_relations: vec![],
        created_block_rules: vec![],
    })
}

/// Block rule DTO for frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockRuleDto {
    /// Rule ID
    pub id: String,
    /// Rule type
    pub rule_type: String,
    /// Description of what is blocked
    pub description: String,
    /// Whether the rule is active
    pub is_active: bool,
    /// Creation timestamp
    pub created_at: String,
    /// Expiration timestamp (if any)
    pub expires_at: Option<String>,
}

// Helper functions

fn parse_block_scope(request: &BlockRelationRequest) -> Result<BlockScope, String> {
    match request.scope.to_lowercase().as_str() {
        "this_pair_only" | "pair" => Ok(BlockScope::ThisPairOnly),
        "source_to_target_tag" | "file_to_tag" => {
            let target_tag_id = request.target_tag_id
                .as_ref()
                .ok_or("target_tag_id required for source_to_target_tag scope")?;
            let tag_uuid = Uuid::parse_str(target_tag_id)
                .map_err(|e| format!("Invalid target_tag_id: {}", e))?;
            Ok(BlockScope::SourceToTargetTag { target_tag_id: tag_uuid })
        }
        "tag_to_tag" => {
            let source_tag_id = request.source_tag_id
                .as_ref()
                .ok_or("source_tag_id required for tag_to_tag scope")?;
            let target_tag_id = request.target_tag_id
                .as_ref()
                .ok_or("target_tag_id required for tag_to_tag scope")?;
            let source_uuid = Uuid::parse_str(source_tag_id)
                .map_err(|e| format!("Invalid source_tag_id: {}", e))?;
            let target_uuid = Uuid::parse_str(target_tag_id)
                .map_err(|e| format!("Invalid target_tag_id: {}", e))?;
            Ok(BlockScope::TagToTag {
                source_tag_id: source_uuid,
                target_tag_id: target_uuid,
            })
        }
        _ => Err(format!("Unknown block scope: {}", request.scope)),
    }
}

fn parse_relation_type(type_str: &str) -> Result<RelationType, String> {
    match type_str.to_lowercase().as_str() {
        "content_similar" | "similar" => Ok(RelationType::ContentSimilar),
        "same_session" | "session" => Ok(RelationType::SameSession),
        "same_project" | "project" => Ok(RelationType::SameProject),
        "same_author" | "author" => Ok(RelationType::SameAuthor),
        "reference" | "ref" => Ok(RelationType::Reference),
        "derivative" => Ok(RelationType::Derivative),
        "workflow" => Ok(RelationType::Workflow),
        "user_defined" | "custom" => Ok(RelationType::UserDefined),
        _ => Err(format!("Unknown relation type: {}", type_str)),
    }
}

fn relation_type_to_string(rel_type: &RelationType) -> String {
    match rel_type {
        RelationType::ContentSimilar => "content_similar".to_string(),
        RelationType::SameSession => "same_session".to_string(),
        RelationType::SameProject => "same_project".to_string(),
        RelationType::SameAuthor => "same_author".to_string(),
        RelationType::Reference => "reference".to_string(),
        RelationType::Derivative => "derivative".to_string(),
        RelationType::Workflow => "workflow".to_string(),
        RelationType::UserDefined => "user_defined".to_string(),
    }
}

fn relation_source_to_string(source: &RelationSource) -> String {
    match source {
        RelationSource::AIGenerated => "ai_generated".to_string(),
        RelationSource::SessionTracking => "session_tracking".to_string(),
        RelationSource::UserManual => "user_manual".to_string(),
        RelationSource::MetadataExtract => "metadata_extract".to_string(),
    }
}

fn user_feedback_to_string(feedback: &UserFeedback) -> String {
    match feedback {
        UserFeedback::None => "none".to_string(),
        UserFeedback::Confirmed => "confirmed".to_string(),
        UserFeedback::Rejected { .. } => "rejected".to_string(),
        UserFeedback::Adjusted { .. } => "adjusted".to_string(),
    }
}
