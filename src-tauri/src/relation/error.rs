//! Relation module error types

use thiserror::Error;
use uuid::Uuid;

/// Result type for relation operations
pub type Result<T> = std::result::Result<T, RelationError>;

/// Relation-related errors
#[derive(Error, Debug)]
pub enum RelationError {
    #[error("Relation not found: {id}")]
    RelationNotFound { id: Uuid },

    #[error("File not found: {id}")]
    FileNotFound { id: Uuid },

    #[error("Relation already exists between {source_id} and {target_id}")]
    RelationAlreadyExists { source_id: Uuid, target_id: Uuid },

    #[error("Cannot create self-relation for file: {id}")]
    SelfRelation { id: Uuid },

    #[error("Invalid relation strength: {value} (must be between 0.0 and 1.0)")]
    InvalidStrength { value: f32 },

    #[error("Block rule not found: {id}")]
    BlockRuleNotFound { id: Uuid },

    #[error("Session not found: {id}")]
    SessionNotFound { id: Uuid },

    #[error("Session already ended: {id}")]
    SessionAlreadyEnded { id: Uuid },

    #[error("Relation is blocked by rule: {rule_id}")]
    RelationBlocked { rule_id: Uuid },

    #[error("Invalid user feedback transition from {from:?} to {to:?}")]
    InvalidFeedbackTransition { from: String, to: String },

    #[error("Vector store error: {0}")]
    VectorStore(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl RelationError {
    /// Check if the error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(self, RelationError::Database(_) | RelationError::VectorStore(_))
    }
}
