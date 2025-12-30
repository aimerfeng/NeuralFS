//! Error types for NeuralFS
//! 
//! Comprehensive error handling with recovery support.
//! Full implementation in task 1.3.

use thiserror::Error;

/// Result type alias for NeuralFS operations
pub type Result<T> = std::result::Result<T, NeuralFSError>;

/// Main error type for NeuralFS
#[derive(Error, Debug)]
pub enum NeuralFSError {
    #[error("Index error: {0}")]
    Index(#[from] IndexError),

    #[error("Search error: {0}")]
    Search(#[from] SearchError),

    #[error("Cloud error: {0}")]
    Cloud(#[from] CloudError),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Database internal error: {0}")]
    DatabaseInternal(#[from] DatabaseError),

    #[error("File system error: {0}")]
    FileSystem(#[from] FileSystemError),

    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("OS integration error: {0}")]
    Os(#[from] OsError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Watcher error: {0}")]
    WatcherError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),
}

/// Indexing-related errors
#[derive(Error, Debug)]
pub enum IndexError {
    #[error("File not found: {path}")]
    FileNotFound { path: String },

    #[error("Unsupported file type: {extension}")]
    UnsupportedFileType { extension: String },

    #[error("Content extraction failed: {reason}")]
    ContentExtractionFailed { reason: String },

    #[error("Embedding generation failed: {reason}")]
    EmbeddingFailed { reason: String },

    #[error("File locked by another process: {path}")]
    FileLocked { path: String },

    #[error("Index corrupted: {reason}")]
    IndexCorrupted { reason: String },

    #[error("Retry limit exceeded for file: {path}")]
    RetryLimitExceeded { path: String },
}

/// Search-related errors
#[derive(Error, Debug)]
pub enum SearchError {
    #[error("Query parsing failed: {reason}")]
    QueryParseFailed { reason: String },

    #[error("Vector search failed: {reason}")]
    VectorSearchFailed { reason: String },

    #[error("Text search failed: {reason}")]
    TextSearchFailed { reason: String },

    #[error("Search timeout after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },

    #[error("Invalid filter: {reason}")]
    InvalidFilter { reason: String },
}

/// Cloud API errors
#[derive(Error, Debug)]
pub enum CloudError {
    #[error("API request failed: {reason}")]
    RequestFailed { reason: String },

    #[error("API rate limit exceeded")]
    RateLimitExceeded,

    #[error("Monthly cost limit reached: ${current:.2} / ${limit:.2}")]
    CostLimitReached { current: f64, limit: f64 },

    #[error("Network unavailable")]
    NetworkUnavailable,

    #[error("API authentication failed")]
    AuthenticationFailed,

    #[error("API response parsing failed: {reason}")]
    ResponseParseFailed { reason: String },

    #[error("Cloud service timeout after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },
}

/// Database errors
#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Connection failed: {reason}")]
    ConnectionFailed { reason: String },

    #[error("Query failed: {reason}")]
    QueryFailed { reason: String },

    #[error("Migration failed: {reason}")]
    MigrationFailed { reason: String },

    #[error("Transaction failed: {reason}")]
    TransactionFailed { reason: String },

    #[error("Database corrupted: {reason}")]
    Corrupted { reason: String },
}

impl From<sqlx::Error> for DatabaseError {
    fn from(err: sqlx::Error) -> Self {
        match &err {
            sqlx::Error::Database(db_err) => {
                DatabaseError::QueryFailed { reason: db_err.to_string() }
            }
            sqlx::Error::PoolTimedOut => {
                DatabaseError::ConnectionFailed { reason: "Pool timed out".to_string() }
            }
            sqlx::Error::PoolClosed => {
                DatabaseError::ConnectionFailed { reason: "Pool closed".to_string() }
            }
            _ => DatabaseError::QueryFailed { reason: err.to_string() }
        }
    }
}

/// File system errors
#[derive(Error, Debug)]
pub enum FileSystemError {
    #[error("Path not found: {path}")]
    PathNotFound { path: String },

    #[error("Permission denied: {path}")]
    PermissionDenied { path: String },

    #[error("Watch failed: {reason}")]
    WatchFailed { reason: String },

    #[error("File read failed: {path}, reason: {reason}")]
    ReadFailed { path: String, reason: String },
}

/// OS integration errors
#[derive(Error, Debug)]
pub enum OsError {
    #[error("Desktop takeover failed: {reason}")]
    DesktopTakeoverFailed { reason: String },

    #[error("Progman window not found")]
    ProgmanNotFound,

    #[error("WorkerW window not found")]
    WorkerWNotFound,

    #[error("Failed to set window parent: {reason}")]
    SetParentFailed { reason: String },

    #[error("Keyboard hook failed: {reason}")]
    KeyboardHookFailed { reason: String },

    #[error("Taskbar control failed: {reason}")]
    TaskbarControlFailed { reason: String },

    #[error("Monitor enumeration failed: {reason}")]
    MonitorEnumFailed { reason: String },

    #[error("Display change handling failed: {reason}")]
    DisplayChangeFailed { reason: String },

    #[error("Window handle invalid: {reason}")]
    InvalidWindowHandle { reason: String },

    #[error("Platform not supported: {platform}")]
    PlatformNotSupported { platform: String },

    #[error("Thumbnail extraction failed: {reason}")]
    ThumbnailExtractionFailed { reason: String },

    #[error("Shell item creation failed for path: {path}")]
    ShellItemCreationFailed { path: String },

    #[error("COM initialization failed: {reason}")]
    ComInitFailed { reason: String },
}

/// Configuration errors
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Config file not found: {path}")]
    FileNotFound { path: String },

    #[error("Config parse failed: {reason}")]
    ParseFailed { reason: String },

    #[error("Invalid config value: {field} = {value}")]
    InvalidValue { field: String, value: String },

    #[error("Config save failed: {reason}")]
    SaveFailed { reason: String },
}

/// Trait for error recovery strategies
pub trait ErrorRecovery {
    /// Check if the error is retryable
    fn is_retryable(&self) -> bool;

    /// Get suggested retry delay in milliseconds
    fn retry_delay_ms(&self) -> Option<u64>;

    /// Get recovery action suggestion
    fn recovery_action(&self) -> RecoveryAction;
}

/// Recovery action suggestions
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryAction {
    /// Retry the operation
    Retry,
    /// Skip this item and continue
    Skip,
    /// Fall back to alternative method
    Fallback,
    /// Notify user and wait for input
    NotifyUser,
    /// Abort the operation
    Abort,
}

impl ErrorRecovery for NeuralFSError {
    fn is_retryable(&self) -> bool {
        match self {
            NeuralFSError::Index(e) => e.is_retryable(),
            NeuralFSError::Search(e) => e.is_retryable(),
            NeuralFSError::Cloud(e) => e.is_retryable(),
            NeuralFSError::Database(_) => true, // sqlx errors are generally retryable
            NeuralFSError::DatabaseInternal(e) => e.is_retryable(),
            NeuralFSError::FileSystem(e) => e.is_retryable(),
            NeuralFSError::Os(e) => e.is_retryable(),
            NeuralFSError::Config(_) => false,
            NeuralFSError::Io(_) => true,
            NeuralFSError::Internal(_) => false,
        }
    }

    fn retry_delay_ms(&self) -> Option<u64> {
        match self {
            NeuralFSError::Index(e) => e.retry_delay_ms(),
            NeuralFSError::Search(e) => e.retry_delay_ms(),
            NeuralFSError::Cloud(e) => e.retry_delay_ms(),
            NeuralFSError::Database(_) => Some(1000), // Default 1 second for sqlx errors
            NeuralFSError::DatabaseInternal(e) => e.retry_delay_ms(),
            NeuralFSError::FileSystem(e) => e.retry_delay_ms(),
            NeuralFSError::Os(e) => e.retry_delay_ms(),
            NeuralFSError::Io(_) => Some(1000),
            _ => None,
        }
    }

    fn recovery_action(&self) -> RecoveryAction {
        match self {
            NeuralFSError::Index(e) => e.recovery_action(),
            NeuralFSError::Search(e) => e.recovery_action(),
            NeuralFSError::Cloud(e) => e.recovery_action(),
            NeuralFSError::Database(_) => RecoveryAction::Retry,
            NeuralFSError::DatabaseInternal(e) => e.recovery_action(),
            NeuralFSError::FileSystem(e) => e.recovery_action(),
            NeuralFSError::Os(e) => e.recovery_action(),
            NeuralFSError::Config(_) => RecoveryAction::NotifyUser,
            NeuralFSError::Io(_) => RecoveryAction::Retry,
            NeuralFSError::Internal(_) => RecoveryAction::Abort,
        }
    }
}

impl ErrorRecovery for IndexError {
    fn is_retryable(&self) -> bool {
        matches!(
            self,
            IndexError::FileLocked { .. }
                | IndexError::ContentExtractionFailed { .. }
                | IndexError::EmbeddingFailed { .. }
        )
    }

    fn retry_delay_ms(&self) -> Option<u64> {
        match self {
            IndexError::FileLocked { .. } => Some(2000), // Wait for file lock release
            IndexError::ContentExtractionFailed { .. } => Some(1000),
            IndexError::EmbeddingFailed { .. } => Some(500),
            _ => None,
        }
    }

    fn recovery_action(&self) -> RecoveryAction {
        match self {
            IndexError::FileNotFound { .. } => RecoveryAction::Skip,
            IndexError::UnsupportedFileType { .. } => RecoveryAction::Skip,
            IndexError::FileLocked { .. } => RecoveryAction::Retry,
            IndexError::ContentExtractionFailed { .. } => RecoveryAction::Retry,
            IndexError::EmbeddingFailed { .. } => RecoveryAction::Retry,
            IndexError::IndexCorrupted { .. } => RecoveryAction::NotifyUser,
            IndexError::RetryLimitExceeded { .. } => RecoveryAction::Skip,
        }
    }
}

impl ErrorRecovery for SearchError {
    fn is_retryable(&self) -> bool {
        matches!(
            self,
            SearchError::VectorSearchFailed { .. }
                | SearchError::TextSearchFailed { .. }
                | SearchError::Timeout { .. }
        )
    }

    fn retry_delay_ms(&self) -> Option<u64> {
        match self {
            SearchError::Timeout { .. } => Some(100),
            SearchError::VectorSearchFailed { .. } => Some(500),
            SearchError::TextSearchFailed { .. } => Some(500),
            _ => None,
        }
    }

    fn recovery_action(&self) -> RecoveryAction {
        match self {
            SearchError::QueryParseFailed { .. } => RecoveryAction::NotifyUser,
            SearchError::InvalidFilter { .. } => RecoveryAction::NotifyUser,
            SearchError::Timeout { .. } => RecoveryAction::Fallback,
            _ => RecoveryAction::Retry,
        }
    }
}

impl ErrorRecovery for CloudError {
    fn is_retryable(&self) -> bool {
        matches!(
            self,
            CloudError::RequestFailed { .. }
                | CloudError::Timeout { .. }
                | CloudError::NetworkUnavailable
        )
    }

    /// Get suggested retry delay in milliseconds.
    /// 
    /// Note: For `RateLimitExceeded`, the actual implementation in `CloudBridge`
    /// should prefer reading the `Retry-After` header from the API response
    /// to get a dynamic wait time. The 60000ms here is a fallback default.
    fn retry_delay_ms(&self) -> Option<u64> {
        match self {
            // Default 60 seconds, but CloudBridge should override with Retry-After header
            CloudError::RateLimitExceeded => Some(60000), // Wait 1 minute
            CloudError::Timeout { .. } => Some(1000),
            CloudError::RequestFailed { .. } => Some(2000),
            CloudError::NetworkUnavailable => Some(5000),
            _ => None,
        }
    }

    fn recovery_action(&self) -> RecoveryAction {
        match self {
            CloudError::CostLimitReached { .. } => RecoveryAction::Fallback,
            CloudError::AuthenticationFailed => RecoveryAction::NotifyUser,
            CloudError::NetworkUnavailable => RecoveryAction::Fallback,
            CloudError::RateLimitExceeded => RecoveryAction::Retry,
            _ => RecoveryAction::Retry,
        }
    }
}

impl ErrorRecovery for DatabaseError {
    fn is_retryable(&self) -> bool {
        matches!(
            self,
            DatabaseError::ConnectionFailed { .. } | DatabaseError::TransactionFailed { .. }
        )
    }

    fn retry_delay_ms(&self) -> Option<u64> {
        match self {
            DatabaseError::ConnectionFailed { .. } => Some(1000),
            DatabaseError::TransactionFailed { .. } => Some(100),
            _ => None,
        }
    }

    fn recovery_action(&self) -> RecoveryAction {
        match self {
            DatabaseError::Corrupted { .. } => RecoveryAction::NotifyUser,
            DatabaseError::MigrationFailed { .. } => RecoveryAction::Abort,
            _ => RecoveryAction::Retry,
        }
    }
}

impl ErrorRecovery for FileSystemError {
    fn is_retryable(&self) -> bool {
        matches!(self, FileSystemError::ReadFailed { .. })
    }

    fn retry_delay_ms(&self) -> Option<u64> {
        match self {
            FileSystemError::ReadFailed { .. } => Some(500),
            _ => None,
        }
    }

    fn recovery_action(&self) -> RecoveryAction {
        match self {
            FileSystemError::PathNotFound { .. } => RecoveryAction::Skip,
            FileSystemError::PermissionDenied { .. } => RecoveryAction::Skip,
            FileSystemError::WatchFailed { .. } => RecoveryAction::NotifyUser,
            FileSystemError::ReadFailed { .. } => RecoveryAction::Retry,
        }
    }
}

impl ErrorRecovery for OsError {
    fn is_retryable(&self) -> bool {
        matches!(
            self,
            OsError::DesktopTakeoverFailed { .. }
                | OsError::SetParentFailed { .. }
                | OsError::DisplayChangeFailed { .. }
                | OsError::ThumbnailExtractionFailed { .. }
                | OsError::ShellItemCreationFailed { .. }
                | OsError::ComInitFailed { .. }
        )
    }

    fn retry_delay_ms(&self) -> Option<u64> {
        match self {
            OsError::DesktopTakeoverFailed { .. } => Some(1000),
            OsError::SetParentFailed { .. } => Some(500),
            OsError::DisplayChangeFailed { .. } => Some(500),
            OsError::ThumbnailExtractionFailed { .. } => Some(100),
            OsError::ShellItemCreationFailed { .. } => Some(100),
            OsError::ComInitFailed { .. } => Some(500),
            _ => None,
        }
    }

    fn recovery_action(&self) -> RecoveryAction {
        match self {
            OsError::PlatformNotSupported { .. } => RecoveryAction::Abort,
            OsError::ProgmanNotFound | OsError::WorkerWNotFound => RecoveryAction::NotifyUser,
            OsError::KeyboardHookFailed { .. } => RecoveryAction::Skip,
            OsError::InvalidWindowHandle { .. } => RecoveryAction::Retry,
            OsError::ThumbnailExtractionFailed { .. } => RecoveryAction::Fallback,
            OsError::ShellItemCreationFailed { .. } => RecoveryAction::Skip,
            OsError::ComInitFailed { .. } => RecoveryAction::Retry,
            _ => RecoveryAction::Retry,
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_error_retryable() {
        // File locked should be retryable
        let err = IndexError::FileLocked {
            path: "/test/file.txt".to_string(),
        };
        assert!(err.is_retryable());
        assert!(err.retry_delay_ms().is_some());
        assert_eq!(err.recovery_action(), RecoveryAction::Retry);

        // File not found should not be retryable
        let err = IndexError::FileNotFound {
            path: "/test/file.txt".to_string(),
        };
        assert!(!err.is_retryable());
        assert!(err.retry_delay_ms().is_none());
        assert_eq!(err.recovery_action(), RecoveryAction::Skip);

        // Unsupported file type should not be retryable
        let err = IndexError::UnsupportedFileType {
            extension: "xyz".to_string(),
        };
        assert!(!err.is_retryable());
        assert_eq!(err.recovery_action(), RecoveryAction::Skip);

        // Embedding failed should be retryable
        let err = IndexError::EmbeddingFailed {
            reason: "VRAM exhausted".to_string(),
        };
        assert!(err.is_retryable());
        assert_eq!(err.recovery_action(), RecoveryAction::Retry);
    }

    #[test]
    fn test_search_error_retryable() {
        // Timeout should be retryable
        let err = SearchError::Timeout { timeout_ms: 5000 };
        assert!(err.is_retryable());
        assert_eq!(err.recovery_action(), RecoveryAction::Fallback);

        // Query parse failed should not be retryable
        let err = SearchError::QueryParseFailed {
            reason: "Invalid syntax".to_string(),
        };
        assert!(!err.is_retryable());
        assert_eq!(err.recovery_action(), RecoveryAction::NotifyUser);

        // Vector search failed should be retryable
        let err = SearchError::VectorSearchFailed {
            reason: "Connection lost".to_string(),
        };
        assert!(err.is_retryable());
        assert_eq!(err.recovery_action(), RecoveryAction::Retry);
    }

    #[test]
    fn test_cloud_error_retryable() {
        // Rate limit should be retryable with long delay
        let err = CloudError::RateLimitExceeded;
        assert!(!err.is_retryable()); // Rate limit is not immediately retryable
        assert_eq!(err.retry_delay_ms(), Some(60000)); // 1 minute wait
        assert_eq!(err.recovery_action(), RecoveryAction::Retry);

        // Cost limit should fallback to local
        let err = CloudError::CostLimitReached {
            current: 10.5,
            limit: 10.0,
        };
        assert!(!err.is_retryable());
        assert_eq!(err.recovery_action(), RecoveryAction::Fallback);

        // Network unavailable should fallback
        let err = CloudError::NetworkUnavailable;
        assert!(err.is_retryable());
        assert_eq!(err.recovery_action(), RecoveryAction::Fallback);

        // Auth failed should notify user
        let err = CloudError::AuthenticationFailed;
        assert!(!err.is_retryable());
        assert_eq!(err.recovery_action(), RecoveryAction::NotifyUser);
    }

    #[test]
    fn test_database_error_retryable() {
        // Connection failed should be retryable
        let err = DatabaseError::ConnectionFailed {
            reason: "Pool exhausted".to_string(),
        };
        assert!(err.is_retryable());
        assert_eq!(err.recovery_action(), RecoveryAction::Retry);

        // Corrupted should notify user
        let err = DatabaseError::Corrupted {
            reason: "Invalid checksum".to_string(),
        };
        assert!(!err.is_retryable());
        assert_eq!(err.recovery_action(), RecoveryAction::NotifyUser);

        // Migration failed should abort
        let err = DatabaseError::MigrationFailed {
            reason: "Schema conflict".to_string(),
        };
        assert!(!err.is_retryable());
        assert_eq!(err.recovery_action(), RecoveryAction::Abort);
    }

    #[test]
    fn test_filesystem_error_retryable() {
        // Path not found should skip
        let err = FileSystemError::PathNotFound {
            path: "/nonexistent".to_string(),
        };
        assert!(!err.is_retryable());
        assert_eq!(err.recovery_action(), RecoveryAction::Skip);

        // Permission denied should skip
        let err = FileSystemError::PermissionDenied {
            path: "/protected".to_string(),
        };
        assert!(!err.is_retryable());
        assert_eq!(err.recovery_action(), RecoveryAction::Skip);

        // Read failed should be retryable
        let err = FileSystemError::ReadFailed {
            path: "/test.txt".to_string(),
            reason: "Temporary lock".to_string(),
        };
        assert!(err.is_retryable());
        assert_eq!(err.recovery_action(), RecoveryAction::Retry);
    }

    #[test]
    fn test_neuralfs_error_conversion() {
        // Test From implementations
        let index_err = IndexError::FileNotFound {
            path: "/test".to_string(),
        };
        let neural_err: NeuralFSError = index_err.into();
        assert!(!neural_err.is_retryable());

        let cloud_err = CloudError::NetworkUnavailable;
        let neural_err: NeuralFSError = cloud_err.into();
        assert!(neural_err.is_retryable());
        assert_eq!(neural_err.recovery_action(), RecoveryAction::Fallback);
    }

    #[test]
    fn test_error_display() {
        let err = IndexError::FileLocked {
            path: "/test/file.txt".to_string(),
        };
        assert!(err.to_string().contains("/test/file.txt"));

        let err = CloudError::CostLimitReached {
            current: 15.50,
            limit: 10.00,
        };
        assert!(err.to_string().contains("15.50"));
        assert!(err.to_string().contains("10.00"));
    }

    #[test]
    fn test_retry_delay_values() {
        // File locked should have 2 second delay
        let err = IndexError::FileLocked {
            path: "/test".to_string(),
        };
        assert_eq!(err.retry_delay_ms(), Some(2000));

        // Cloud timeout should have 1 second delay
        let err = CloudError::Timeout { timeout_ms: 5000 };
        assert_eq!(err.retry_delay_ms(), Some(1000));

        // Network unavailable should have 5 second delay
        let err = CloudError::NetworkUnavailable;
        assert_eq!(err.retry_delay_ms(), Some(5000));
    }
}
