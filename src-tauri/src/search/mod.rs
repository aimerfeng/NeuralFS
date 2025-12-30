//! Full-text search module for NeuralFS
//!
//! This module provides:
//! - Multi-language tokenization (Chinese, Japanese, English)
//! - Tantivy-based full-text indexing
//! - Schema version control and migration

pub mod tokenizer;
pub mod text_index;

#[cfg(test)]
mod tests;

pub use tokenizer::{
    JiebaTokenizer, MultilingualTokenizer, SimpleTokenizer, Language, LanguageDetector,
};
pub use text_index::{TextIndex, TextIndexConfig, TextIndexError};

#[cfg(feature = "japanese")]
pub use tokenizer::LinderaTokenizer;
