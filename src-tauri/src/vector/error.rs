//! Vector store error types

use thiserror::Error;

/// Vector store specific errors
#[derive(Error, Debug)]
pub enum VectorError {
    #[error("Failed to initialize vector store: {reason}")]
    InitializationFailed { reason: String },

    #[error("Collection not found: {name}")]
    CollectionNotFound { name: String },

    #[error("Failed to create collection: {reason}")]
    CollectionCreationFailed { reason: String },

    #[error("Failed to upsert vectors: {reason}")]
    UpsertFailed { reason: String },

    #[error("Failed to search vectors: {reason}")]
    SearchFailed { reason: String },

    #[error("Failed to delete vectors: {reason}")]
    DeleteFailed { reason: String },

    #[error("Invalid vector dimension: expected {expected}, got {actual}")]
    InvalidDimension { expected: u64, actual: u64 },

    #[error("Lock file cleanup failed: {path}")]
    LockFileCleanupFailed { path: String },

    #[error("Storage path error: {reason}")]
    StoragePathError { reason: String },

    #[error("Serialization error: {reason}")]
    SerializationError { reason: String },

    #[error("Point not found: {id}")]
    PointNotFound { id: u64 },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl VectorError {
    /// Check if the error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            VectorError::UpsertFailed { .. }
                | VectorError::SearchFailed { .. }
                | VectorError::DeleteFailed { .. }
                | VectorError::LockFileCleanupFailed { .. }
        )
    }

    /// Get suggested retry delay in milliseconds
    pub fn retry_delay_ms(&self) -> Option<u64> {
        match self {
            VectorError::UpsertFailed { .. } => Some(500),
            VectorError::SearchFailed { .. } => Some(200),
            VectorError::DeleteFailed { .. } => Some(500),
            VectorError::LockFileCleanupFailed { .. } => Some(1000),
            _ => None,
        }
    }
}
