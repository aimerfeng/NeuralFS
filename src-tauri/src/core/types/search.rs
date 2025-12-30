//! Search types
//! 
//! Defines structures for search requests and responses.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

use super::chunk::{ChunkType, ContentChunk};
use super::file::{FileRecord, FileType};
use super::tag::Tag;

/// Search request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    /// User's original query
    pub query: String,
    
    /// Parsed intent
    pub intent: Option<SearchIntent>,
    
    /// Filter conditions
    pub filters: SearchFilters,
    
    /// Pagination
    pub pagination: Pagination,
    
    /// Whether to enable cloud enhancement
    pub enable_cloud: bool,
    
    /// Request ID (for tracking)
    pub request_id: Uuid,
    
    /// Request timestamp
    pub timestamp: DateTime<Utc>,
}

/// Search intent classification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SearchIntent {
    /// Find file
    FindFile {
        /// File type hint
        file_type_hint: Option<FileType>,
        /// Time range hint
        time_hint: Option<TimeRange>,
    },
    
    /// Find content segment
    FindContent {
        /// Expected content type
        content_type: Option<ChunkType>,
        /// Whether precise location is needed
        need_location: bool,
    },
    
    /// Ambiguous query (needs clarification)
    Ambiguous {
        /// Possible interpretations
        possible_intents: Vec<SearchIntent>,
        /// Suggested clarification questions
        clarification_questions: Vec<String>,
    },
}

/// Search filters
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchFilters {
    /// File type filter
    pub file_types: Option<Vec<FileType>>,
    
    /// Tag filter (AND logic)
    pub tags: Option<Vec<Uuid>>,
    
    /// Exclude tags
    pub exclude_tags: Option<Vec<Uuid>>,
    
    /// Time range
    pub time_range: Option<TimeRange>,
    
    /// Path prefix
    pub path_prefix: Option<PathBuf>,
    
    /// Minimum similarity score
    pub min_score: f32,
    
    /// Exclude private files
    pub exclude_private: bool,
}

/// Time range for filtering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
}

/// Pagination parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pagination {
    pub offset: u32,
    pub limit: u32,
}

impl Default for Pagination {
    fn default() -> Self {
        Self { offset: 0, limit: 20 }
    }
}

/// Search response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    /// Request ID
    pub request_id: Uuid,
    
    /// Response status
    pub status: SearchStatus,
    
    /// Search results
    pub results: Vec<SearchResult>,
    
    /// Total match count (for pagination)
    pub total_count: u64,
    
    /// Whether there are more results
    pub has_more: bool,
    
    /// Search duration (milliseconds)
    pub duration_ms: u64,
    
    /// Data sources
    pub sources: Vec<ResultSource>,
    
    /// Clarification suggestions (if intent is ambiguous)
    pub clarifications: Option<Vec<Clarification>>,
}

/// Search status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SearchStatus {
    Success,
    PartialSuccess, // Partial success (e.g., cloud timeout)
    NeedsClarity,   // Needs user clarification
    NoResults,
    Error,
}

/// Result source
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ResultSource {
    LocalVector,    // Local vector search
    LocalTag,       // Local tag matching
    CloudEnhanced,  // Cloud enhanced
}

/// Single search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Result type
    pub result_type: SearchResultType,
    
    /// File information
    pub file: FileRecord,
    
    /// Matched content chunk (if content search)
    pub matched_chunk: Option<ContentChunk>,
    
    /// Similarity score (0.0 - 1.0)
    pub score: f32,
    
    /// Preview content
    pub preview: ResultPreview,
    
    /// Highlight information
    pub highlights: Vec<Highlight>,
    
    /// Related tags
    pub tags: Vec<Tag>,
    
    /// Data source
    pub source: ResultSource,
}

/// Search result type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SearchResultType {
    File,         // File-level result
    ContentChunk, // Content segment result
}

/// Result preview
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultPreview {
    /// Preview type
    pub preview_type: PreviewType,
    
    /// Preview content
    pub content: PreviewContent,
}

/// Preview type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PreviewType {
    Text,
    Image,
    Thumbnail,
    Metadata,
}

/// Preview content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PreviewContent {
    Text {
        snippet: String,
        full_text: Option<String>,
    },
    Image {
        thumbnail_base64: String,
        width: u32,
        height: u32,
    },
    Metadata {
        entries: HashMap<String, String>,
    },
}

/// Highlight information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Highlight {
    /// Highlight start position
    pub start: u32,
    /// Highlight end position
    pub end: u32,
    /// Highlight type
    pub highlight_type: HighlightType,
}

/// Highlight type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum HighlightType {
    ExactMatch,    // Exact match
    SemanticMatch, // Semantic match
    KeywordMatch,  // Keyword match
}

/// Clarification suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clarification {
    /// Clarification question
    pub question: String,
    
    /// Available options
    pub options: Vec<ClarificationOption>,
}

/// Clarification option
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClarificationOption {
    /// Option text
    pub text: String,
    
    /// Search intent after selection
    pub intent: SearchIntent,
    
    /// Estimated result count
    pub estimated_count: Option<u64>,
}
