//! Full-text search module for NeuralFS
//!
//! This module provides:
//! - Multi-language tokenization (Chinese, Japanese, English)
//! - Tantivy-based full-text indexing
//! - Schema version control and migration
//! - Intent parsing for file-level vs content-level search
//! - Hybrid search combining vector and BM25 search

pub mod tokenizer;
pub mod text_index;
pub mod intent;
pub mod hybrid;

#[cfg(test)]
mod tests;

pub use tokenizer::{
    JiebaTokenizer, MultilingualTokenizer, SimpleTokenizer, Language, LanguageDetector,
};
pub use text_index::{TextIndex, TextIndexConfig, TextIndexError};
pub use intent::{IntentParser, IntentParseResult, IntentCategory, TimeHint};
pub use hybrid::{
    HybridSearchEngine, HybridSearchConfig, HybridSearchError, HybridSearchFilters,
    QueryType, ScoredResult, SearchSource, classify_query, apply_filters,
};

#[cfg(feature = "japanese")]
pub use tokenizer::LinderaTokenizer;
