//! Search Commands for NeuralFS
//!
//! Provides Tauri commands for semantic search functionality:
//! - search_files: Execute semantic search with intent parsing
//! - get_search_suggestions: Get search suggestions based on partial query
//!
//! **Validates: Requirements 2.1, 2.2**

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::types::file::FileType;
use crate::core::types::search::{
    Pagination, SearchFilters, SearchIntent, SearchRequest, SearchResponse, SearchResult,
    SearchStatus, TimeRange, ResultSource,
};
use crate::search::intent::{IntentParser, IntentParseResult};
use crate::search::hybrid::{HybridSearchEngine, QueryType, classify_query};

/// Search request from frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchFilesRequest {
    /// User's search query
    pub query: String,
    /// Optional file type filter
    pub file_types: Option<Vec<String>>,
    /// Optional tag IDs filter
    pub tag_ids: Option<Vec<String>>,
    /// Optional time range filter
    pub time_range: Option<TimeRangeDto>,
    /// Minimum score threshold (0.0 - 1.0)
    pub min_score: Option<f32>,
    /// Whether to exclude private files
    pub exclude_private: Option<bool>,
    /// Whether to enable cloud enhancement
    pub enable_cloud: Option<bool>,
    /// Pagination offset
    pub offset: Option<u32>,
    /// Pagination limit
    pub limit: Option<u32>,
}

/// Time range DTO for frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRangeDto {
    /// Start timestamp (ISO 8601)
    pub start: Option<String>,
    /// End timestamp (ISO 8601)
    pub end: Option<String>,
}

/// Search response for frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchFilesResponse {
    /// Request ID for tracking
    pub request_id: String,
    /// Search status
    pub status: String,
    /// Search results
    pub results: Vec<SearchResultDto>,
    /// Total count of matching results
    pub total_count: u64,
    /// Whether there are more results
    pub has_more: bool,
    /// Search duration in milliseconds
    pub duration_ms: u64,
    /// Parsed intent information
    pub intent: Option<IntentInfoDto>,
    /// Clarification suggestions if query is ambiguous
    pub clarifications: Option<Vec<ClarificationDto>>,
}

/// Search result DTO for frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultDto {
    /// File ID
    pub file_id: String,
    /// File path
    pub path: String,
    /// File name
    pub filename: String,
    /// File type
    pub file_type: String,
    /// Relevance score (0.0 - 1.0)
    pub score: f32,
    /// Preview snippet
    pub preview: Option<String>,
    /// Matched chunk ID (if segment-level result)
    pub chunk_id: Option<String>,
    /// Result source (local_vector, local_tag, cloud_enhanced)
    pub source: String,
    /// Associated tag names
    pub tags: Vec<String>,
}

/// Intent information DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentInfoDto {
    /// Intent category (file, content, ambiguous)
    pub category: String,
    /// Confidence score
    pub confidence: f32,
    /// Extracted keywords
    pub keywords: Vec<String>,
    /// File type hint if detected
    pub file_type_hint: Option<String>,
    /// Time hint if detected
    pub time_hint: Option<String>,
}

/// Clarification suggestion DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClarificationDto {
    /// Clarification question
    pub question: String,
    /// Available options
    pub options: Vec<ClarificationOptionDto>,
}

/// Clarification option DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClarificationOptionDto {
    /// Option text
    pub text: String,
    /// Estimated result count
    pub estimated_count: Option<u64>,
}

/// Search suggestion response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchSuggestion {
    /// Suggestion text
    pub text: String,
    /// Suggestion type (recent, popular, tag, file_type)
    pub suggestion_type: String,
    /// Optional icon or indicator
    pub icon: Option<String>,
}

/// Execute semantic search
///
/// This command performs hybrid search combining vector (semantic) and BM25 (keyword) search.
/// It also parses user intent to determine if the search is file-level or content-level.
///
/// # Arguments
/// * `request` - Search request containing query and filters
///
/// # Returns
/// Search response with results, intent info, and optional clarifications
#[tauri::command]
pub async fn search_files(request: SearchFilesRequest) -> Result<SearchFilesResponse, String> {
    let start_time = std::time::Instant::now();
    let request_id = Uuid::now_v7();

    // Parse intent
    let intent_parser = IntentParser::new();
    let intent_result = intent_parser.parse(&request.query);

    // Classify query type for search strategy
    let query_type = classify_query(&request.query);

    // Build search filters
    let filters = build_search_filters(&request)?;

    // Create pagination
    let pagination = Pagination {
        offset: request.offset.unwrap_or(0),
        limit: request.limit.unwrap_or(20),
    };

    // For now, return a mock response since we don't have full integration
    // In production, this would call the actual search engine
    let duration_ms = start_time.elapsed().as_millis() as u64;

    // Build intent info
    let intent_info = build_intent_info(&intent_result, query_type);

    // Build clarifications if intent is ambiguous
    let clarifications = if intent_result.is_ambiguous {
        Some(build_clarifications(&intent_result))
    } else {
        None
    };

    // Determine status
    let status = if intent_result.is_ambiguous {
        "needs_clarity"
    } else {
        "success"
    };

    Ok(SearchFilesResponse {
        request_id: request_id.to_string(),
        status: status.to_string(),
        results: vec![], // Would be populated by actual search
        total_count: 0,
        has_more: false,
        duration_ms,
        intent: Some(intent_info),
        clarifications,
    })
}

/// Get search suggestions based on partial query
///
/// Returns suggestions including:
/// - Recent searches
/// - Popular searches
/// - Tag-based suggestions
/// - File type suggestions
///
/// # Arguments
/// * `query` - Partial search query
/// * `limit` - Maximum number of suggestions to return
///
/// # Returns
/// List of search suggestions
#[tauri::command]
pub async fn get_search_suggestions(
    query: String,
    limit: Option<u32>,
) -> Result<Vec<SearchSuggestion>, String> {
    let limit = limit.unwrap_or(10) as usize;
    let query_lower = query.to_lowercase();

    let mut suggestions = Vec::new();

    // Add file type suggestions if query matches
    let file_type_suggestions = get_file_type_suggestions(&query_lower);
    for suggestion in file_type_suggestions.into_iter().take(3) {
        suggestions.push(suggestion);
    }

    // Add tag-based suggestions (mock for now)
    if query_lower.len() >= 2 {
        suggestions.push(SearchSuggestion {
            text: format!("tag:{}", query),
            suggestion_type: "tag".to_string(),
            icon: Some("🏷️".to_string()),
        });
    }

    // Add query completion suggestions
    if !query.is_empty() {
        suggestions.push(SearchSuggestion {
            text: format!("{} files", query),
            suggestion_type: "completion".to_string(),
            icon: Some("📄".to_string()),
        });
        suggestions.push(SearchSuggestion {
            text: format!("{} content", query),
            suggestion_type: "completion".to_string(),
            icon: Some("📝".to_string()),
        });
    }

    // Limit results
    suggestions.truncate(limit);

    Ok(suggestions)
}

// Helper functions

fn build_search_filters(request: &SearchFilesRequest) -> Result<SearchFilters, String> {
    let mut filters = SearchFilters::default();

    // Parse file types
    if let Some(ref types) = request.file_types {
        let parsed_types: Vec<FileType> = types
            .iter()
            .filter_map(|t| parse_file_type(t))
            .collect();
        if !parsed_types.is_empty() {
            filters.file_types = Some(parsed_types);
        }
    }

    // Parse tag IDs
    if let Some(ref tag_ids) = request.tag_ids {
        let parsed_ids: Vec<Uuid> = tag_ids
            .iter()
            .filter_map(|id| Uuid::parse_str(id).ok())
            .collect();
        if !parsed_ids.is_empty() {
            filters.tags = Some(parsed_ids);
        }
    }

    // Parse time range
    if let Some(ref time_range) = request.time_range {
        let start = time_range.start.as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc));
        let end = time_range.end.as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc));
        
        if start.is_some() || end.is_some() {
            filters.time_range = Some(TimeRange { start, end });
        }
    }

    // Set min score
    filters.min_score = request.min_score.unwrap_or(0.3);

    // Set exclude private
    filters.exclude_private = request.exclude_private.unwrap_or(false);

    Ok(filters)
}

fn parse_file_type(type_str: &str) -> Option<FileType> {
    match type_str.to_lowercase().as_str() {
        "pdf" => Some(FileType::Pdf),
        "text" | "txt" | "text_document" => Some(FileType::TextDocument),
        "office" | "doc" | "docx" | "office_document" => Some(FileType::OfficeDocument),
        "image" | "img" | "png" | "jpg" | "jpeg" => Some(FileType::Image),
        "video" | "mp4" | "avi" => Some(FileType::Video),
        "audio" | "mp3" | "wav" => Some(FileType::Audio),
        "code" | "source" => Some(FileType::Code),
        "model" | "3d" | "model_3d" => Some(FileType::Model3D),
        "archive" | "zip" | "rar" => Some(FileType::Archive),
        _ => None,
    }
}

fn build_intent_info(intent_result: &IntentParseResult, query_type: QueryType) -> IntentInfoDto {
    let category = match &intent_result.intent {
        SearchIntent::FindFile { .. } => "file",
        SearchIntent::FindContent { .. } => "content",
        SearchIntent::Ambiguous { .. } => "ambiguous",
    };

    let (file_type_hint, time_hint) = match &intent_result.intent {
        SearchIntent::FindFile { file_type_hint, time_hint } => {
            let ft = file_type_hint.as_ref().map(|t| format!("{:?}", t));
            let th = time_hint.as_ref().map(|_| "detected".to_string());
            (ft, th)
        }
        _ => (None, None),
    };

    IntentInfoDto {
        category: category.to_string(),
        confidence: intent_result.confidence,
        keywords: intent_result.extracted_keywords.clone(),
        file_type_hint,
        time_hint,
    }
}

fn build_clarifications(intent_result: &IntentParseResult) -> Vec<ClarificationDto> {
    if let SearchIntent::Ambiguous { clarification_questions, .. } = &intent_result.intent {
        clarification_questions
            .iter()
            .map(|q| ClarificationDto {
                question: q.clone(),
                options: vec![
                    ClarificationOptionDto {
                        text: "Find files".to_string(),
                        estimated_count: None,
                    },
                    ClarificationOptionDto {
                        text: "Find content".to_string(),
                        estimated_count: None,
                    },
                ],
            })
            .collect()
    } else {
        vec![]
    }
}

fn get_file_type_suggestions(query: &str) -> Vec<SearchSuggestion> {
    let file_types = [
        ("pdf", "PDF Documents", "📕"),
        ("doc", "Word Documents", "📘"),
        ("image", "Images", "🖼️"),
        ("video", "Videos", "🎬"),
        ("code", "Source Code", "💻"),
        ("audio", "Audio Files", "🎵"),
    ];

    file_types
        .iter()
        .filter(|(keyword, _, _)| keyword.contains(query) || query.contains(*keyword))
        .map(|(_, name, icon)| SearchSuggestion {
            text: format!("type:{}", name.to_lowercase().replace(' ', "_")),
            suggestion_type: "file_type".to_string(),
            icon: Some(icon.to_string()),
        })
        .collect()
}
