//! Error types for the embedding engine

use thiserror::Error;

/// Result type for embedding operations
pub type EmbeddingResult<T> = Result<T, EmbeddingError>;

/// Errors that can occur during embedding operations
#[derive(Error, Debug)]
pub enum EmbeddingError {
    /// Model file not found
    #[error("Model not found: {path}")]
    ModelNotFound { path: String },
    
    /// Model is not loaded
    #[error("Model not loaded: {model_type}")]
    ModelNotLoaded { model_type: String },
    
    /// Model is currently loading
    #[error("Model is loading: {model_type}")]
    ModelLoading { model_type: String },
    
    /// Model loading failed
    #[error("Model loading failed: {reason}")]
    ModelLoadFailed { reason: String },
    
    /// Model is in failed state
    #[error("Model in failed state: {model_type}, reason: {reason}")]
    ModelFailed { model_type: String, reason: String },
    
    /// VRAM insufficient for model
    #[error("VRAM insufficient: need {needed_mb}MB, available {available_mb}MB")]
    VRAMInsufficient { needed_mb: u64, available_mb: u64 },
    
    /// Inference failed
    #[error("Inference failed: {reason}")]
    InferenceFailed { reason: String },
    
    /// Tokenization failed
    #[error("Tokenization failed: {reason}")]
    TokenizationFailed { reason: String },
    
    /// Image processing failed
    #[error("Image processing failed: {reason}")]
    ImageProcessingFailed { reason: String },
    
    /// Invalid input
    #[error("Invalid input: {reason}")]
    InvalidInput { reason: String },
    
    /// ONNX runtime error
    #[error("ONNX runtime error: {0}")]
    OnnxError(String),
    
    /// IO error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    /// Model download failed
    #[error("Model download failed: {reason}")]
    DownloadFailed { reason: String },
    
    /// Configuration error
    #[error("Configuration error: {reason}")]
    ConfigError { reason: String },
}

impl From<ort::Error> for EmbeddingError {
    fn from(err: ort::Error) -> Self {
        EmbeddingError::OnnxError(err.to_string())
    }
}

impl EmbeddingError {
    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            EmbeddingError::ModelLoading { .. }
                | EmbeddingError::InferenceFailed { .. }
                | EmbeddingError::VRAMInsufficient { .. }
        )
    }
    
    /// Get suggested retry delay in milliseconds
    pub fn retry_delay_ms(&self) -> Option<u64> {
        match self {
            EmbeddingError::ModelLoading { .. } => Some(1000), // Wait for model to load
            EmbeddingError::InferenceFailed { .. } => Some(100),
            EmbeddingError::VRAMInsufficient { .. } => Some(2000), // Wait for VRAM to free up
            _ => None,
        }
    }
    
    /// Check if this error should trigger graceful degradation
    pub fn should_degrade(&self) -> bool {
        matches!(
            self,
            EmbeddingError::ModelNotFound { .. }
                | EmbeddingError::ModelNotLoaded { .. }
                | EmbeddingError::ModelFailed { .. }
                | EmbeddingError::VRAMInsufficient { .. }
        )
    }
}
