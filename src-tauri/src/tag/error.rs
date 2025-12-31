//! Tag module error types

use thiserror::Error;
use uuid::Uuid;

/// Result type for tag operations
pub type Result<T> = std::result::Result<T, TagError>;

/// Tag-related errors
#[derive(Error, Debug)]
pub enum TagError {
    #[error("Tag not found: {id}")]
    TagNotFound { id: Uuid },

    #[error("File not found: {id}")]
    FileNotFound { id: Uuid },

    #[error("Tag already exists: {name}")]
    TagAlreadyExists { name: String },

    #[error("Cannot delete system tag: {name}")]
    CannotDeleteSystemTag { name: String },

    #[error("Tag hierarchy depth exceeded: max {max_depth} levels allowed")]
    HierarchyDepthExceeded { max_depth: u32 },

    #[error("Circular hierarchy detected: {tag_id} -> {parent_id}")]
    CircularHierarchy { tag_id: Uuid, parent_id: Uuid },

    #[error("Invalid tag name: {reason}")]
    InvalidTagName { reason: String },

    #[error("Tag relation already exists: file {file_id} -> tag {tag_id}")]
    RelationAlreadyExists { file_id: Uuid, tag_id: Uuid },

    #[error("Tag relation not found: file {file_id} -> tag {tag_id}")]
    RelationNotFound { file_id: Uuid, tag_id: Uuid },

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl TagError {
    /// Check if the error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(self, TagError::Database(_))
    }
}
