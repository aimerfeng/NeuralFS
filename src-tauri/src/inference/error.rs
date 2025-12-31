//! Error types for the inference module

use thiserror::Error;

/// Inference-specific errors
#[derive(Debug, Error)]
pub enum InferenceError {
    /// Embedding generation failed
    #[error("Embedding generation failed: {reason}")]
    EmbeddingFailed { reason: String },

    /// Intent parsing failed
    #[error("Intent parsing failed: {reason}")]
    IntentParseFailed { reason: String },

    /// Cloud API error
    #[error("Cloud API error: {reason}")]
    CloudApiError { reason: String },

    /// Rate limit exceeded
    #[error("Rate limit exceeded, retry after {retry_after_secs} seconds")]
    RateLimitExceeded { retry_after_secs: u64 },

    /// Cost limit reached
    #[error("Monthly cost limit reached: ${current:.2} / ${limit:.2}")]
    CostLimitReached { current: f64, limit: f64 },

    /// Network error
    #[error("Network error: {reason}")]
    NetworkError { reason: String },

    /// Timeout error
    #[error("Inference timeout after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },

    /// Cache error
    #[error("Cache error: {reason}")]
    CacheError { reason: String },

    /// Configuration error
    #[error("Configuration error: {reason}")]
    ConfigError { reason: String },

    /// Cloud service unavailable
    #[error("Cloud service unavailable: {reason}")]
    CloudUnavailable { reason: String },

    /// Serialization error
    #[error("Serialization error: {reason}")]
    SerializationError { reason: String },

    /// Internal error
    #[error("Internal error: {reason}")]
    Internal { reason: String },

    /// Database error
    #[error("Database error: {reason}")]
    DatabaseError { reason: String },
}

impl From<reqwest::Error> for InferenceError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            InferenceError::Timeout { timeout_ms: 0 }
        } else if err.is_connect() {
            InferenceError::NetworkError {
                reason: format!("Connection failed: {}", err),
            }
        } else {
            InferenceError::CloudApiError {
                reason: err.to_string(),
            }
        }
    }
}

impl From<serde_json::Error> for InferenceError {
    fn from(err: serde_json::Error) -> Self {
        InferenceError::SerializationError {
            reason: err.to_string(),
        }
    }
}

/// Result type for inference operations
pub type InferenceResult<T> = Result<T, InferenceError>;
