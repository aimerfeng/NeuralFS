//! Vector database module using Qdrant embedded
//!
//! This module provides vector storage and retrieval functionality for semantic search.
//! It uses Qdrant in embedded mode for zero-dependency local deployment.

mod store;
mod config;
mod error;

#[cfg(test)]
mod tests;

pub use store::VectorStore;
pub use config::{VectorStoreConfig, HnswConfig, OptimizerConfig, Distance};
pub use error::VectorError;

/// Payload field names for vector points
pub mod payload_fields {
    /// File UUID as string
    pub const FILE_ID: &str = "file_id";
    /// Chunk UUID as string
    pub const CHUNK_ID: &str = "chunk_id";
    /// File type enum value
    pub const FILE_TYPE: &str = "file_type";
    /// Array of tag UUIDs
    pub const TAG_IDS: &str = "tag_ids";
    /// Creation timestamp (ISO 8601)
    pub const CREATED_AT: &str = "created_at";
    /// Modification timestamp (ISO 8601)
    pub const MODIFIED_AT: &str = "modified_at";
    /// Privacy level enum value
    pub const PRIVACY_LEVEL: &str = "privacy_level";
}
