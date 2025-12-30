//! Common types for the inference module

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

use crate::core::types::search::SearchIntent;
use crate::core::types::tag::Tag;

/// Inference request containing query and context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    /// Unique request identifier
    pub request_id: Uuid,
    
    /// User's search query
    pub query: String,
    
    /// Context information for inference
    pub context: InferenceContext,
    
    /// Inference options
    pub options: InferenceOptions,
    
    /// Request timestamp
    pub timestamp: DateTime<Utc>,
}

impl InferenceRequest {
    /// Create a new inference request
    pub fn new(query: String, context: InferenceContext, options: InferenceOptions) -> Self {
        Self {
            request_id: Uuid::now_v7(),
            query,
            context,
            options,
            timestamp: Utc::now(),
        }
    }
    
    /// Create a simple request with default context and options
    pub fn simple(query: String) -> Self {
        Self::new(
            query,
            InferenceContext::default(),
            InferenceOptions::default(),
        )
    }
}

/// Context information for inference
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InferenceContext {
    /// Relevant tags (from local analysis)
    pub relevant_tags: Vec<Tag>,
    
    /// File structure context
    pub file_structure: Option<FileStructureContext>,
    
    /// User history (recent searches, recent files)
    pub user_history: UserHistoryContext,
    
    /// Current session context
    pub session_context: SessionContext,
}

/// File structure context for enhanced prompts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStructureContext {
    /// Summary of file structure
    pub summary: String,
    
    /// Top-level directories
    pub top_directories: Vec<String>,
    
    /// Total file count
    pub total_files: u64,
    
    /// File type distribution
    pub file_type_counts: Vec<(String, u64)>,
}

/// User history context
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserHistoryContext {
    /// Recently accessed files
    pub recent_files: Vec<RecentFile>,
    
    /// Recent search queries
    pub recent_searches: Vec<String>,
    
    /// Frequently used tags
    pub frequent_tags: Vec<Tag>,
}

/// Recent file information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentFile {
    /// File ID
    pub file_id: Uuid,
    
    /// File name (without path)
    pub filename: String,
    
    /// File path
    pub path: PathBuf,
    
    /// Last access time
    pub accessed_at: DateTime<Utc>,
}

/// Session context
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionContext {
    /// Session ID
    pub session_id: Option<Uuid>,
    
    /// Files opened in this session
    pub session_files: Vec<Uuid>,
    
    /// Session start time
    pub session_start: Option<DateTime<Utc>>,
}

/// Inference options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceOptions {
    /// Whether to enable cloud inference
    pub enable_cloud: bool,
    
    /// Cloud timeout in milliseconds
    pub cloud_timeout_ms: u64,
    
    /// Local model type to use
    pub local_model: LocalModelType,
    
    /// Cloud model type to use (if cloud enabled)
    pub cloud_model: Option<CloudModelType>,
    
    /// Whether to use cache
    pub use_cache: bool,
    
    /// Maximum results to return
    pub max_results: usize,
}

impl Default for InferenceOptions {
    fn default() -> Self {
        Self {
            enable_cloud: true,
            cloud_timeout_ms: 500,
            local_model: LocalModelType::Fast,
            cloud_model: Some(CloudModelType::GPT4oMini),
            use_cache: true,
            max_results: 20,
        }
    }
}

/// Local model type selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocalModelType {
    /// MiniLM-L6 (fast, 384 dimensions)
    Fast,
    /// all-MiniLM-L12 (balanced)
    Balanced,
    /// BGE-base (accurate)
    Accurate,
}

impl Default for LocalModelType {
    fn default() -> Self {
        Self::Fast
    }
}

/// Cloud model type selection
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloudModelType {
    /// OpenAI GPT-4o-mini
    GPT4oMini,
    /// Anthropic Claude Haiku
    ClaudeHaiku,
    /// Custom model endpoint
    Custom(String),
}

impl std::fmt::Display for CloudModelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CloudModelType::GPT4oMini => write!(f, "gpt-4o-mini"),
            CloudModelType::ClaudeHaiku => write!(f, "claude-3-haiku-20240307"),
            CloudModelType::Custom(name) => write!(f, "{}", name),
        }
    }
}

/// Inference response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResponse {
    /// Request ID
    pub request_id: Uuid,
    
    /// Parsed search intent
    pub intent: SearchIntent,
    
    /// Query embedding vector
    pub query_embedding: Vec<f32>,
    
    /// Cloud-enhanced understanding (if available)
    pub cloud_understanding: Option<CloudUnderstanding>,
    
    /// Whether cloud was used
    pub cloud_enhanced: bool,
    
    /// Total inference duration in milliseconds
    pub duration_ms: u64,
    
    /// Data sources used
    pub sources: Vec<InferenceSource>,
}

/// Cloud understanding result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudUnderstanding {
    /// Refined intent from cloud
    pub refined_intent: Option<String>,
    
    /// Suggested search terms
    pub suggested_terms: Vec<String>,
    
    /// Confidence score
    pub confidence: f32,
    
    /// Raw response (for debugging)
    pub raw_response: Option<String>,
}

/// Source of inference data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InferenceSource {
    /// Local embedding engine
    LocalEmbedding,
    /// Local intent parser
    LocalIntent,
    /// Local tag matching
    LocalTagMatch,
    /// Cloud API
    Cloud,
    /// Cache hit
    Cache,
}
