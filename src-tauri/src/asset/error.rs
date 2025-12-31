//! Asset server error types

use thiserror::Error;

/// Asset server error type
#[derive(Error, Debug)]
pub enum AssetError {
    #[error("Invalid session token")]
    InvalidToken,

    #[error("CSRF protection: invalid origin '{origin}'")]
    InvalidOrigin { origin: String },

    #[error("CSRF protection: invalid referer '{referer}'")]
    InvalidReferer { referer: String },

    #[error("Asset not found: {uuid}")]
    NotFound { uuid: String },

    #[error("Server bind failed: {reason}")]
    BindFailed { reason: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Internal server error: {reason}")]
    Internal { reason: String },
}

impl AssetError {
    /// Check if this error should result in a 403 Forbidden response
    pub fn is_forbidden(&self) -> bool {
        matches!(
            self,
            AssetError::InvalidToken | AssetError::InvalidOrigin { .. } | AssetError::InvalidReferer { .. }
        )
    }

    /// Check if this error should result in a 404 Not Found response
    pub fn is_not_found(&self) -> bool {
        matches!(self, AssetError::NotFound { .. })
    }
}
