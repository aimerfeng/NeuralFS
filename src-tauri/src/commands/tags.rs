//! Tag Commands for NeuralFS
//!
//! Provides Tauri commands for tag management:
//! - get_tags: Get all tags or tags for a specific file
//! - add_tag: Add a tag to a file
//! - remove_tag: Remove a tag from a file
//! - confirm_tag: Confirm an AI-generated tag
//! - reject_tag: Reject an AI-generated tag
//!
//! **Validates: Requirements 5.1, Human-in-the-Loop**

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::types::{Tag, TagType, TagSource, FileTagRelation};
use crate::tag::{TagCommand, TagCorrectionResult};

/// Tag DTO for frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagDto {
    /// Tag ID
    pub id: String,
    /// Tag name
    pub name: String,
    /// Tag display name (localized)
    pub display_name: Option<String>,
    /// Parent tag ID
    pub parent_id: Option<String>,
    /// Tag type
    pub tag_type: String,
    /// Tag color (hex)
    pub color: String,
    /// Tag icon
    pub icon: Option<String>,
    /// Whether this is a system tag
    pub is_system: bool,
    /// Usage count
    pub usage_count: u64,
}

/// File-tag relation DTO for frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTagDto {
    /// Relation ID
    pub id: String,
    /// File ID
    pub file_id: String,
    /// Tag information
    pub tag: TagDto,
    /// Source of the tag assignment
    pub source: String,
    /// Confidence score (for AI-generated)
    pub confidence: Option<f32>,
    /// Whether confirmed by user
    pub is_confirmed: bool,
    /// Whether rejected by user
    pub is_rejected: bool,
}

/// Request to add a tag
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddTagRequest {
    /// File ID
    pub file_id: String,
    /// Tag ID
    pub tag_id: String,
}

/// Request to remove a tag
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveTagRequest {
    /// File ID
    pub file_id: String,
    /// Tag ID
    pub tag_id: String,
}

/// Request to confirm a tag
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmTagRequest {
    /// File ID
    pub file_id: String,
    /// Tag ID
    pub tag_id: String,
}

/// Request to reject a tag
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectTagRequest {
    /// File ID
    pub file_id: String,
    /// Tag ID
    pub tag_id: String,
    /// Whether to block similar tags
    pub block_similar: Option<bool>,
}

/// Request to create a new tag
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTagRequest {
    /// Tag name
    pub name: String,
    /// Parent tag ID
    pub parent_id: Option<String>,
    /// Tag type
    pub tag_type: Option<String>,
    /// Tag color (hex)
    pub color: Option<String>,
}

/// Tag operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagOperationResult {
    /// Whether the operation succeeded
    pub success: bool,
    /// Human-readable message
    pub message: String,
    /// Affected file IDs
    pub affected_files: Vec<String>,
    /// Affected tag IDs
    pub affected_tags: Vec<String>,
    /// Created tag (if applicable)
    pub created_tag: Option<TagDto>,
}

/// Get all tags
///
/// Returns all tags in the system, optionally filtered by parent.
///
/// # Arguments
/// * `parent_id` - Optional parent tag ID to filter by
///
/// # Returns
/// List of tags
#[tauri::command]
pub async fn get_tags(parent_id: Option<String>) -> Result<Vec<TagDto>, String> {
    // Parse parent ID if provided
    let _parent_uuid = parent_id
        .as_ref()
        .map(|id| Uuid::parse_str(id))
        .transpose()
        .map_err(|e| format!("Invalid parent_id: {}", e))?;

    // In production, this would query the database
    // For now, return mock data
    Ok(vec![
        TagDto {
            id: Uuid::now_v7().to_string(),
            name: "Work".to_string(),
            display_name: Some("工作".to_string()),
            parent_id: None,
            tag_type: "Category".to_string(),
            color: "#3B82F6".to_string(),
            icon: Some("💼".to_string()),
            is_system: true,
            usage_count: 42,
        },
        TagDto {
            id: Uuid::now_v7().to_string(),
            name: "Personal".to_string(),
            display_name: Some("个人".to_string()),
            parent_id: None,
            tag_type: "Category".to_string(),
            color: "#10B981".to_string(),
            icon: Some("🏠".to_string()),
            is_system: true,
            usage_count: 28,
        },
    ])
}

/// Get tags for a specific file
///
/// Returns all tags associated with a file, including AI-generated suggestions.
///
/// # Arguments
/// * `file_id` - File ID
///
/// # Returns
/// List of file-tag relations
#[tauri::command]
pub async fn get_file_tags(file_id: String) -> Result<Vec<FileTagDto>, String> {
    let _file_uuid = Uuid::parse_str(&file_id)
        .map_err(|e| format!("Invalid file_id: {}", e))?;

    // In production, this would query the database
    // For now, return empty list
    Ok(vec![])
}

/// Add a tag to a file
///
/// Manually adds a tag to a file. The tag source will be set to Manual.
///
/// # Arguments
/// * `request` - Add tag request
///
/// # Returns
/// Operation result
#[tauri::command]
pub async fn add_tag(request: AddTagRequest) -> Result<TagOperationResult, String> {
    let file_uuid = Uuid::parse_str(&request.file_id)
        .map_err(|e| format!("Invalid file_id: {}", e))?;
    let tag_uuid = Uuid::parse_str(&request.tag_id)
        .map_err(|e| format!("Invalid tag_id: {}", e))?;

    // In production, this would use TagCorrectionService
    // For now, return success
    Ok(TagOperationResult {
        success: true,
        message: "Tag added successfully".to_string(),
        affected_files: vec![file_uuid.to_string()],
        affected_tags: vec![tag_uuid.to_string()],
        created_tag: None,
    })
}

/// Remove a tag from a file
///
/// Removes a tag association from a file.
///
/// # Arguments
/// * `request` - Remove tag request
///
/// # Returns
/// Operation result
#[tauri::command]
pub async fn remove_tag(request: RemoveTagRequest) -> Result<TagOperationResult, String> {
    let file_uuid = Uuid::parse_str(&request.file_id)
        .map_err(|e| format!("Invalid file_id: {}", e))?;
    let tag_uuid = Uuid::parse_str(&request.tag_id)
        .map_err(|e| format!("Invalid tag_id: {}", e))?;

    // In production, this would use TagCorrectionService
    Ok(TagOperationResult {
        success: true,
        message: "Tag removed successfully".to_string(),
        affected_files: vec![file_uuid.to_string()],
        affected_tags: vec![tag_uuid.to_string()],
        created_tag: None,
    })
}

/// Confirm an AI-generated tag
///
/// Marks an AI-generated tag as confirmed by the user.
/// This feedback is used to improve future tag suggestions.
///
/// # Arguments
/// * `request` - Confirm tag request
///
/// # Returns
/// Operation result
#[tauri::command]
pub async fn confirm_tag(request: ConfirmTagRequest) -> Result<TagOperationResult, String> {
    let file_uuid = Uuid::parse_str(&request.file_id)
        .map_err(|e| format!("Invalid file_id: {}", e))?;
    let tag_uuid = Uuid::parse_str(&request.tag_id)
        .map_err(|e| format!("Invalid tag_id: {}", e))?;

    // In production, this would use TagCorrectionService
    Ok(TagOperationResult {
        success: true,
        message: "Tag confirmed".to_string(),
        affected_files: vec![file_uuid.to_string()],
        affected_tags: vec![tag_uuid.to_string()],
        created_tag: None,
    })
}

/// Reject an AI-generated tag
///
/// Marks an AI-generated tag as rejected by the user.
/// Optionally blocks similar tags from being suggested in the future.
///
/// # Arguments
/// * `request` - Reject tag request
///
/// # Returns
/// Operation result
#[tauri::command]
pub async fn reject_tag(request: RejectTagRequest) -> Result<TagOperationResult, String> {
    let file_uuid = Uuid::parse_str(&request.file_id)
        .map_err(|e| format!("Invalid file_id: {}", e))?;
    let tag_uuid = Uuid::parse_str(&request.tag_id)
        .map_err(|e| format!("Invalid tag_id: {}", e))?;

    let block_similar = request.block_similar.unwrap_or(false);

    // In production, this would use TagCorrectionService
    let message = if block_similar {
        "Tag rejected and similar tags blocked"
    } else {
        "Tag rejected"
    };

    Ok(TagOperationResult {
        success: true,
        message: message.to_string(),
        affected_files: vec![file_uuid.to_string()],
        affected_tags: vec![tag_uuid.to_string()],
        created_tag: None,
    })
}

/// Create a new tag
///
/// Creates a new custom tag.
///
/// # Arguments
/// * `request` - Create tag request
///
/// # Returns
/// Operation result with created tag
#[tauri::command]
pub async fn create_tag(request: CreateTagRequest) -> Result<TagOperationResult, String> {
    let parent_uuid = request.parent_id
        .as_ref()
        .map(|id| Uuid::parse_str(id))
        .transpose()
        .map_err(|e| format!("Invalid parent_id: {}", e))?;

    let tag_type = request.tag_type
        .as_ref()
        .map(|t| parse_tag_type(t))
        .unwrap_or(TagType::Custom);

    let new_tag_id = Uuid::now_v7();

    // In production, this would use TagManager
    let created_tag = TagDto {
        id: new_tag_id.to_string(),
        name: request.name.clone(),
        display_name: None,
        parent_id: parent_uuid.map(|u| u.to_string()),
        tag_type: format!("{:?}", tag_type),
        color: request.color.unwrap_or_else(|| "#6B7280".to_string()),
        icon: None,
        is_system: false,
        usage_count: 0,
    };

    Ok(TagOperationResult {
        success: true,
        message: "Tag created successfully".to_string(),
        affected_files: vec![],
        affected_tags: vec![new_tag_id.to_string()],
        created_tag: Some(created_tag),
    })
}

// Helper functions

fn parse_tag_type(type_str: &str) -> TagType {
    match type_str.to_lowercase().as_str() {
        "category" => TagType::Category,
        "filetype" | "file_type" => TagType::FileType,
        "project" => TagType::Project,
        "status" => TagType::Status,
        "autogenerated" | "auto_generated" => TagType::AutoGenerated,
        _ => TagType::Custom,
    }
}

fn tag_to_dto(tag: &Tag) -> TagDto {
    TagDto {
        id: tag.id.to_string(),
        name: tag.name.clone(),
        display_name: tag.display_name.get("zh-CN").cloned(),
        parent_id: tag.parent_id.map(|u| u.to_string()),
        tag_type: format!("{:?}", tag.tag_type),
        color: tag.color.clone(),
        icon: tag.icon.clone(),
        is_system: tag.is_system,
        usage_count: tag.usage_count,
    }
}

fn tag_source_to_string(source: &TagSource) -> String {
    match source {
        TagSource::Manual => "manual".to_string(),
        TagSource::AIGenerated => "ai_generated".to_string(),
        TagSource::Inherited => "inherited".to_string(),
        TagSource::Imported => "imported".to_string(),
    }
}
