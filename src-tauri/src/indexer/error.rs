//! Error types for the indexer module

use std::path::PathBuf;
use thiserror::Error;
use uuid::Uuid;

use super::TaskStatus;

/// Errors that can occur during indexing operations
#[derive(Error, Debug, Clone)]
pub enum IndexError {
    #[error("File not found: {path}")]
    FileNotFound { path: PathBuf },

    #[error("File is locked by another process: {path}")]
    FileLocked { path: PathBuf },

    #[error("Unsupported file type: {extension}")]
    UnsupportedFileType { extension: String },

    #[error("Content extraction failed: {reason}")]
    ContentExtractionFailed { reason: String },

    #[error("Embedding generation failed: {reason}")]
    EmbeddingFailed { reason: String },

    #[error("Vector storage failed: {reason}")]
    StorageFailed { reason: String },

    #[error("IO error: {reason}")]
    IoError { reason: String },

    #[error("Task timeout")]
    Timeout,

    #[error("Task not found: {task_id}")]
    TaskNotFound { task_id: Uuid },

    #[error("Index corrupted: {reason}")]
    IndexCorrupted { reason: String },

    #[error("Queue full")]
    QueueFull,

    #[error("Invalid state transition from {from:?} to {to:?}")]
    InvalidStateTransition { from: TaskStatus, to: TaskStatus },
}

impl IndexError {
    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            IndexError::FileLocked { .. }
                | IndexError::IoError { .. }
                | IndexError::Timeout
                | IndexError::StorageFailed { .. }
                | IndexError::EmbeddingFailed { .. }
        )
    }

    /// Check if this is a file lock error
    pub fn is_file_locked(&self) -> bool {
        matches!(self, IndexError::FileLocked { .. })
    }
}

impl From<std::io::Error> for IndexError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => IndexError::FileNotFound {
                path: PathBuf::new(),
            },
            std::io::ErrorKind::PermissionDenied => IndexError::FileLocked {
                path: PathBuf::new(),
            },
            _ => IndexError::IoError {
                reason: err.to_string(),
            },
        }
    }
}
