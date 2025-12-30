//! Hybrid Search Engine for NeuralFS
//!
//! This module provides:
//! - Combined vector search (semantic) + BM25 (keyword) search
//! - Query type classification (exact keyword, natural language, mixed)
//! - Search filtering by file type, tags, time range, and privacy level
//! - Score normalization and result merging
//!
//! **Validates: Requirements 2.2, 2.3, Hybrid Search Logic**

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::core::types::file::FileType;
use crate::core::types::search::{
    Pagination, ResultSource, SearchFilters, SearchRequest, SearchResponse, SearchResult,
    SearchResultType, SearchStatus, TimeRange,
};
use crate::search::text_index::{SearchFilters as TextSearchFilters, SearchResult as TextSearchResult, TextIndex};
use crate::vector::store::{SearchFilter as VectorSearchFilter, SearchResult as VectorSearchResult, VectorStore};

/// Error types for hybrid search operations
#[derive(Error, Debug)]
pub enum HybridSearchError {
    #[error("Vector search error: {0}")]
    VectorSearch(String),

    #[error("Text search error: {0}")]
    TextSearch(String),

    #[error("Query embedding failed: {0}")]
    QueryEmbedding(String),

    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    #[error("Search timeout after {0}ms")]
    Timeout(u64),
}

/// Query type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryType {
    /// Exact keyword search (error codes, file names, constants)
    ExactKeyword,
    /// Natural language description
    NaturalLanguage,
    /// Mixed query (combination of both)
    Mixed,
}

/// Configuration for hybrid search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSearchConfig {
    /// Weight for vector (semantic) search results
    pub vector_weight: f32,
    /// Weight for BM25 (keyword) search results
    pub bm25_weight: f32,
    /// Boost factor for exact matches
    pub exact_match_boost: f32,
    /// Boost factor for filename matches
    pub filename_match_boost: f32,
    /// Minimum vector score threshold
    pub min_vector_score: f32,
    /// Minimum BM25 score threshold
    pub min_bm25_score: f32,
    /// Maximum results to return
    pub max_results: usize,
    /// Timeout in milliseconds
    pub timeout_ms: u64,
}

impl Default for HybridSearchConfig {
    fn default() -> Self {
        Self {
            vector_weight: 0.6,
            bm25_weight: 0.4,
            exact_match_boost: 2.0,
            filename_match_boost: 1.5,
            min_vector_score: 0.3,
            min_bm25_score: 0.1,
            max_results: 100,
            timeout_ms: 5000,
        }
    }
}

impl HybridSearchConfig {
    /// Validate that weights sum to 1.0
    pub fn validate(&self) -> Result<(), HybridSearchError> {
        let weight_sum = self.vector_weight + self.bm25_weight;
        if (weight_sum - 1.0).abs() > 0.001 {
            return Err(HybridSearchError::InvalidQuery(format!(
                "Weights must sum to 1.0, got {}",
                weight_sum
            )));
        }
        Ok(())
    }

    /// Create config with custom weights (normalized to sum to 1.0)
    pub fn with_weights(vector_weight: f32, bm25_weight: f32) -> Self {
        let sum = vector_weight + bm25_weight;
        Self {
            vector_weight: vector_weight / sum,
            bm25_weight: bm25_weight / sum,
            ..Default::default()
        }
    }
}

/// Intermediate scored result for merging
#[derive(Debug, Clone)]
pub struct ScoredResult {
    /// File UUID
    pub file_id: Uuid,
    /// Chunk UUID (if segment-level result)
    pub chunk_id: Option<Uuid>,
    /// Combined score
    pub score: f32,
    /// Vector search score (if available)
    pub vector_score: Option<f32>,
    /// BM25 search score (if available)
    pub bm25_score: Option<f32>,
    /// Source of the result
    pub source: SearchSource,
    /// Filename (for boost calculation)
    pub filename: Option<String>,
    /// Associated tags
    pub tags: Vec<String>,
}

/// Source of a search result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchSource {
    /// From vector search only
    Vector,
    /// From BM25 search only
    BM25,
    /// From both searches (merged)
    Both,
}

/// Hybrid search engine combining vector and BM25 search
pub struct HybridSearchEngine {
    /// Configuration
    config: HybridSearchConfig,
}

impl HybridSearchEngine {
    /// Create a new HybridSearchEngine with default configuration
    pub fn new() -> Self {
        Self {
            config: HybridSearchConfig::default(),
        }
    }

    /// Create a new HybridSearchEngine with custom configuration
    pub fn with_config(config: HybridSearchConfig) -> Result<Self, HybridSearchError> {
        config.validate()?;
        Ok(Self { config })
    }

    /// Get the current configuration
    pub fn config(&self) -> &HybridSearchConfig {
        &self.config
    }

    /// Classify the query type to determine search strategy
    pub fn classify_query(&self, query: &str) -> QueryType {
        classify_query(query)
    }

    /// Get adjusted weights based on query type
    pub fn get_adjusted_weights(&self, query_type: QueryType) -> (f32, f32) {
        match query_type {
            QueryType::ExactKeyword => {
                // Exact keywords benefit more from BM25
                (0.2, 0.8)
            }
            QueryType::NaturalLanguage => {
                // Natural language benefits more from vector search
                (0.8, 0.2)
            }
            QueryType::Mixed => {
                // Use configured weights
                (self.config.vector_weight, self.config.bm25_weight)
            }
        }
    }


    /// Merge vector and BM25 search results with weighted scoring
    ///
    /// # Arguments
    /// * `vector_results` - Results from vector (semantic) search
    /// * `bm25_results` - Results from BM25 (keyword) search
    /// * `weights` - Tuple of (vector_weight, bm25_weight)
    ///
    /// # Returns
    /// Merged and sorted results with combined scores
    pub fn merge_results(
        &self,
        vector_results: Vec<VectorSearchResult>,
        bm25_results: Vec<TextSearchResult>,
        weights: (f32, f32),
    ) -> Vec<ScoredResult> {
        let (vector_weight, bm25_weight) = weights;
        let mut result_map: HashMap<Uuid, ScoredResult> = HashMap::new();

        // Normalize vector scores to [0, 1] range
        let max_vector_score = vector_results
            .iter()
            .map(|r| r.score)
            .fold(0.0f32, |a, b| a.max(b));
        let vector_normalizer = if max_vector_score > 0.0 {
            max_vector_score
        } else {
            1.0
        };

        // Normalize BM25 scores to [0, 1] range
        let max_bm25_score = bm25_results
            .iter()
            .map(|r| r.score)
            .fold(0.0f32, |a, b| a.max(b));
        let bm25_normalizer = if max_bm25_score > 0.0 {
            max_bm25_score
        } else {
            1.0
        };

        // Process vector results
        for vr in vector_results {
            let file_id = vr.file_id().unwrap_or_else(Uuid::nil);
            if file_id.is_nil() {
                continue;
            }

            let normalized_score = vr.score / vector_normalizer;
            let weighted_score = normalized_score * vector_weight;

            result_map.insert(
                file_id,
                ScoredResult {
                    file_id,
                    chunk_id: vr.chunk_id(),
                    score: weighted_score,
                    vector_score: Some(normalized_score),
                    bm25_score: None,
                    source: SearchSource::Vector,
                    filename: None,
                    tags: Vec::new(),
                },
            );
        }

        // Process BM25 results and merge with vector results
        for br in bm25_results {
            let normalized_score = br.score / bm25_normalizer;
            let weighted_score = normalized_score * bm25_weight;

            if let Some(existing) = result_map.get_mut(&br.file_id) {
                // Merge with existing vector result
                existing.score += weighted_score;
                existing.bm25_score = Some(normalized_score);
                existing.source = SearchSource::Both;
                existing.filename = br.filename.clone();
                existing.tags = br.tags.clone();
            } else {
                // New result from BM25 only
                result_map.insert(
                    br.file_id,
                    ScoredResult {
                        file_id: br.file_id,
                        chunk_id: br.chunk_id,
                        score: weighted_score,
                        vector_score: None,
                        bm25_score: Some(normalized_score),
                        source: SearchSource::BM25,
                        filename: br.filename,
                        tags: br.tags,
                    },
                );
            }
        }

        // Convert to vector and sort by score descending
        let mut results: Vec<ScoredResult> = result_map.into_values().collect();
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        results
    }

    /// Apply exact match boost to results
    pub fn apply_exact_match_boost(&self, results: &mut [ScoredResult], query: &str) {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();

        for result in results.iter_mut() {
            let mut boost = 1.0f32;

            // Check filename match
            if let Some(ref filename) = result.filename {
                let filename_lower = filename.to_lowercase();
                
                // Exact filename match
                if filename_lower.contains(&query_lower) {
                    boost *= self.config.filename_match_boost;
                }
                
                // Partial word match in filename
                for word in &query_words {
                    if filename_lower.contains(word) {
                        boost *= 1.1;
                    }
                }
            }

            // Check tag match
            for tag in &result.tags {
                let tag_lower = tag.to_lowercase();
                if query_words.iter().any(|w| tag_lower.contains(w)) {
                    boost *= 1.2;
                }
            }

            result.score *= boost;
        }

        // Re-sort after boosting
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    }

    /// Filter results based on minimum score thresholds
    pub fn filter_by_score(&self, results: Vec<ScoredResult>) -> Vec<ScoredResult> {
        results
            .into_iter()
            .filter(|r| {
                // Keep if combined score is above threshold
                // or if individual scores meet their thresholds
                let vector_ok = r.vector_score.map_or(true, |s| s >= self.config.min_vector_score);
                let bm25_ok = r.bm25_score.map_or(true, |s| s >= self.config.min_bm25_score);
                vector_ok || bm25_ok
            })
            .collect()
    }

    /// Limit results to max_results
    pub fn limit_results(&self, results: Vec<ScoredResult>) -> Vec<ScoredResult> {
        results.into_iter().take(self.config.max_results).collect()
    }
}

impl Default for HybridSearchEngine {
    fn default() -> Self {
        Self::new()
    }
}


// ============================================================================
// Query Classification
// ============================================================================

/// Classify a query into ExactKeyword, NaturalLanguage, or Mixed
///
/// This function analyzes the query to determine the best search strategy:
/// - ExactKeyword: Error codes, file names, constants (favor BM25)
/// - NaturalLanguage: Descriptive queries (favor vector search)
/// - Mixed: Combination of both (use balanced weights)
pub fn classify_query(query: &str) -> QueryType {
    // Check for exact keyword patterns
    if is_exact_keyword_query(query) {
        return QueryType::ExactKeyword;
    }

    // Check for natural language patterns
    if is_natural_language_query(query) {
        return QueryType::NaturalLanguage;
    }

    QueryType::Mixed
}

/// Check if query matches exact keyword patterns
fn is_exact_keyword_query(query: &str) -> bool {
    // Hexadecimal error codes (e.g., 0x80070005)
    if regex::Regex::new(r"0x[0-9a-fA-F]+")
        .map(|r| r.is_match(query))
        .unwrap_or(false)
    {
        return true;
    }

    // Long numeric sequences (e.g., error codes, IDs)
    if regex::Regex::new(r"\d{4,}")
        .map(|r| r.is_match(query))
        .unwrap_or(false)
    {
        return true;
    }

    // All-caps constants (e.g., ERROR_ACCESS_DENIED)
    if regex::Regex::new(r"[A-Z_]{3,}")
        .map(|r| r.is_match(query))
        .unwrap_or(false)
    {
        return true;
    }

    // File name patterns (e.g., report.pdf, image.png)
    if regex::Regex::new(r"\w+\.\w{2,4}")
        .map(|r| r.is_match(query))
        .unwrap_or(false)
    {
        return true;
    }

    // Quoted exact search
    if query.contains('"') || query.contains('"') || query.contains('"') {
        return true;
    }

    // Path-like patterns
    if query.contains('/') || query.contains('\\') {
        return true;
    }

    false
}

/// Check if query is natural language
fn is_natural_language_query(query: &str) -> bool {
    let words: Vec<&str> = query.split_whitespace().collect();

    // Natural language queries typically have 3+ words
    if words.len() >= 3 {
        return true;
    }

    // Check for question words
    let question_words = ["what", "where", "how", "why", "when", "which", "who"];
    let query_lower = query.to_lowercase();
    if question_words.iter().any(|w| query_lower.starts_with(w)) {
        return true;
    }

    // Check for descriptive phrases
    let descriptive_patterns = [
        "find", "search", "look for", "show me", "get", "locate",
        "找", "搜索", "查找", "显示", "获取",
    ];
    if descriptive_patterns.iter().any(|p| query_lower.contains(p)) {
        return true;
    }

    false
}

// ============================================================================
// Search Filters
// ============================================================================

/// Extended search filters for hybrid search
#[derive(Debug, Clone, Default)]
pub struct HybridSearchFilters {
    /// File type filter
    pub file_types: Option<Vec<FileType>>,
    /// Tag IDs filter (AND logic)
    pub tag_ids: Option<Vec<Uuid>>,
    /// Exclude tag IDs
    pub exclude_tag_ids: Option<Vec<Uuid>>,
    /// Time range filter
    pub time_range: Option<TimeRange>,
    /// Minimum score threshold
    pub min_score: Option<f32>,
    /// Exclude private files
    pub exclude_private: bool,
    /// Path prefix filter
    pub path_prefix: Option<String>,
}

impl HybridSearchFilters {
    /// Create new empty filters
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by file types
    pub fn with_file_types(mut self, types: Vec<FileType>) -> Self {
        self.file_types = Some(types);
        self
    }

    /// Filter by tag IDs
    pub fn with_tag_ids(mut self, ids: Vec<Uuid>) -> Self {
        self.tag_ids = Some(ids);
        self
    }

    /// Exclude specific tags
    pub fn with_exclude_tags(mut self, ids: Vec<Uuid>) -> Self {
        self.exclude_tag_ids = Some(ids);
        self
    }

    /// Filter by time range
    pub fn with_time_range(mut self, start: Option<DateTime<Utc>>, end: Option<DateTime<Utc>>) -> Self {
        self.time_range = Some(TimeRange { start, end });
        self
    }

    /// Set minimum score threshold
    pub fn with_min_score(mut self, score: f32) -> Self {
        self.min_score = Some(score);
        self
    }

    /// Exclude private files
    pub fn exclude_private(mut self) -> Self {
        self.exclude_private = true;
        self
    }

    /// Filter by path prefix
    pub fn with_path_prefix(mut self, prefix: String) -> Self {
        self.path_prefix = Some(prefix);
        self
    }

    /// Convert to vector store filter
    pub fn to_vector_filter(&self) -> VectorSearchFilter {
        let mut filter = VectorSearchFilter::new();

        if let Some(ref types) = self.file_types {
            let type_strings: Vec<String> = types.iter().map(|t| format!("{:?}", t)).collect();
            filter = filter.with_file_types(type_strings);
        }

        if let Some(ref ids) = self.tag_ids {
            filter = filter.with_tag_ids(ids.clone());
        }

        if self.exclude_private {
            filter = filter.exclude_private();
        }

        filter
    }

    /// Convert to text index filter
    pub fn to_text_filter(&self) -> TextSearchFilters {
        let mut filter = TextSearchFilters::default();

        if let Some(ref types) = self.file_types {
            let type_strings: Vec<String> = types.iter().map(|t| format!("{:?}", t)).collect();
            filter.file_types = Some(type_strings);
        }

        if let Some(ref range) = self.time_range {
            if let Some(start) = range.start {
                filter.min_modified_at = Some(start.timestamp() as u64);
            }
            if let Some(end) = range.end {
                filter.max_modified_at = Some(end.timestamp() as u64);
            }
        }

        filter
    }

    /// Check if a result passes all filters
    pub fn matches(&self, result: &ScoredResult) -> bool {
        // Check minimum score
        if let Some(min) = self.min_score {
            if result.score < min {
                return false;
            }
        }

        // Other filters are applied at the search level
        true
    }
}

/// Apply filters to scored results
pub fn apply_filters(results: Vec<ScoredResult>, filters: &HybridSearchFilters) -> Vec<ScoredResult> {
    results.into_iter().filter(|r| filters.matches(r)).collect()
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_query_classification_exact_keyword() {
        // Hex error codes
        assert_eq!(classify_query("0x80070005"), QueryType::ExactKeyword);
        assert_eq!(classify_query("error 0xDEADBEEF"), QueryType::ExactKeyword);

        // Long numbers
        assert_eq!(classify_query("12345678"), QueryType::ExactKeyword);

        // Constants
        assert_eq!(classify_query("ERROR_ACCESS_DENIED"), QueryType::ExactKeyword);
        assert_eq!(classify_query("MAX_PATH_LENGTH"), QueryType::ExactKeyword);

        // File names
        assert_eq!(classify_query("report.pdf"), QueryType::ExactKeyword);
        assert_eq!(classify_query("image.png"), QueryType::ExactKeyword);

        // Quoted search
        assert_eq!(classify_query("\"exact phrase\""), QueryType::ExactKeyword);

        // Paths
        assert_eq!(classify_query("C:\\Users\\test"), QueryType::ExactKeyword);
        assert_eq!(classify_query("/home/user/docs"), QueryType::ExactKeyword);
    }

    #[test]
    fn test_query_classification_natural_language() {
        // Multi-word queries
        assert_eq!(
            classify_query("find documents about machine learning"),
            QueryType::NaturalLanguage
        );
        assert_eq!(
            classify_query("show me recent project files"),
            QueryType::NaturalLanguage
        );

        // Question words
        assert_eq!(
            classify_query("where is the report"),
            QueryType::NaturalLanguage
        );
        assert_eq!(
            classify_query("how to configure settings"),
            QueryType::NaturalLanguage
        );

        // Chinese queries
        assert_eq!(classify_query("找文件"), QueryType::NaturalLanguage);
        assert_eq!(classify_query("搜索文档"), QueryType::NaturalLanguage);
    }

    #[test]
    fn test_query_classification_mixed() {
        // Short queries without special patterns
        assert_eq!(classify_query("test"), QueryType::Mixed);
        assert_eq!(classify_query("data"), QueryType::Mixed);
        assert_eq!(classify_query("AI"), QueryType::Mixed);
    }

    #[test]
    fn test_config_validation() {
        // Valid config
        let config = HybridSearchConfig::default();
        assert!(config.validate().is_ok());

        // Invalid config (weights don't sum to 1.0)
        let invalid_config = HybridSearchConfig {
            vector_weight: 0.5,
            bm25_weight: 0.3,
            ..Default::default()
        };
        assert!(invalid_config.validate().is_err());
    }

    #[test]
    fn test_config_with_weights() {
        let config = HybridSearchConfig::with_weights(3.0, 1.0);
        assert!((config.vector_weight - 0.75).abs() < 0.001);
        assert!((config.bm25_weight - 0.25).abs() < 0.001);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_adjusted_weights() {
        let engine = HybridSearchEngine::new();

        let (v, b) = engine.get_adjusted_weights(QueryType::ExactKeyword);
        assert!((v - 0.2).abs() < 0.001);
        assert!((b - 0.8).abs() < 0.001);

        let (v, b) = engine.get_adjusted_weights(QueryType::NaturalLanguage);
        assert!((v - 0.8).abs() < 0.001);
        assert!((b - 0.2).abs() < 0.001);

        let (v, b) = engine.get_adjusted_weights(QueryType::Mixed);
        assert!((v - 0.6).abs() < 0.001);
        assert!((b - 0.4).abs() < 0.001);
    }

    #[test]
    fn test_filters_builder() {
        let filters = HybridSearchFilters::new()
            .with_file_types(vec![FileType::Pdf, FileType::TextDocument])
            .with_min_score(0.5)
            .exclude_private();

        assert!(filters.file_types.is_some());
        assert_eq!(filters.file_types.as_ref().unwrap().len(), 2);
        assert_eq!(filters.min_score, Some(0.5));
        assert!(filters.exclude_private);
    }

    #[test]
    fn test_merge_results_empty() {
        let engine = HybridSearchEngine::new();
        let results = engine.merge_results(vec![], vec![], (0.6, 0.4));
        assert!(results.is_empty());
    }
}
