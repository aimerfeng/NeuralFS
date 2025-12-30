//! Core data types for NeuralFS
//! 
//! This module defines the fundamental data structures used throughout the application.

pub mod file;
pub mod chunk;
pub mod tag;
pub mod relation;
pub mod search;

// Re-export commonly used types
pub use file::{FileRecord, FileType, IndexStatus, PrivacyLevel};
pub use chunk::{ContentChunk, ChunkType, ChunkLocation};
pub use tag::{Tag, TagType, FileTagRelation, TagSource};
pub use relation::{FileRelation, RelationType, UserFeedback, RelationBlockRule};
