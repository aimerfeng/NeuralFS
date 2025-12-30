//! Content Parser Module
//!
//! Provides content extraction from various file formats for indexing.
//! Implements the ContentParser trait with format-specific parsers.
//!
//! Supported formats:
//! - Text files (TXT, MD, JSON)
//! - PDF documents
//! - Code files with syntax analysis

mod text;
mod pdf;
mod code;
#[cfg(test)]
mod tests;

pub use text::TextParser;
pub use pdf::PdfParser;
pub use code::CodeParser;

use crate::core::types::chunk::{ChunkLocation, ChunkType, ContentChunk};
use crate::core::types::file::FileType;
use async_trait::async_trait;
use chrono::Utc;
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur during content parsing
#[derive(Error, Debug)]
pub enum ParseError {
    #[error("File not found: {path}")]
    FileNotFound { path: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Unsupported file type: {extension}")]
    UnsupportedFileType { extension: String },

    #[error("Parse failed: {reason}")]
    ParseFailed { reason: String },

    #[error("Encoding error: {reason}")]
    EncodingError { reason: String },

    #[error("File corrupted or invalid: {reason}")]
    CorruptedFile { reason: String },
}

/// Result of parsing a file
#[derive(Debug, Clone)]
pub struct ParseResult {
    /// Extracted text content
    pub text: String,
    /// Content chunks for indexing
    pub chunks: Vec<ContentChunk>,
    /// Metadata extracted from the file
    pub metadata: ParseMetadata,
}

/// Metadata extracted during parsing
#[derive(Debug, Clone, Default)]
pub struct ParseMetadata {
    /// Document title (if available)
    pub title: Option<String>,
    /// Document author (if available)
    pub author: Option<String>,
    /// Creation date (if available)
    pub created_date: Option<String>,
    /// Number of pages (for PDF)
    pub page_count: Option<u32>,
    /// Language detected
    pub language: Option<String>,
    /// Word count
    pub word_count: usize,
    /// Character count
    pub char_count: usize,
}

/// Configuration for content parsing
#[derive(Debug, Clone)]
pub struct ParseConfig {
    /// Maximum chunk size in characters
    pub max_chunk_size: usize,
    /// Minimum chunk size in characters
    pub min_chunk_size: usize,
    /// Overlap between chunks in characters
    pub chunk_overlap: usize,
    /// Whether to preserve formatting
    pub preserve_formatting: bool,
    /// Whether to extract metadata
    pub extract_metadata: bool,
}

impl Default for ParseConfig {
    fn default() -> Self {
        Self {
            max_chunk_size: 1000,
            min_chunk_size: 100,
            chunk_overlap: 100,
            preserve_formatting: true,
            extract_metadata: true,
        }
    }
}

/// Trait for content parsers
#[async_trait]
pub trait ContentParser: Send + Sync {
    /// Parse a file and extract content
    async fn parse(&self, path: &Path, config: &ParseConfig) -> Result<ParseResult, ParseError>;

    /// Check if this parser supports the given file type
    fn supports(&self, file_type: FileType) -> bool;

    /// Get supported file extensions
    fn supported_extensions(&self) -> &[&str];
}

/// Main content parser that delegates to format-specific parsers
pub struct ContentParserService {
    text_parser: TextParser,
    pdf_parser: PdfParser,
    code_parser: CodeParser,
    config: ParseConfig,
}

impl ContentParserService {
    /// Create a new content parser service with default config
    pub fn new() -> Self {
        Self::with_config(ParseConfig::default())
    }

    /// Create a new content parser service with custom config
    pub fn with_config(config: ParseConfig) -> Self {
        Self {
            text_parser: TextParser::new(),
            pdf_parser: PdfParser::new(),
            code_parser: CodeParser::new(),
            config,
        }
    }

    /// Parse a file based on its type
    pub async fn parse(&self, path: &Path) -> Result<ParseResult, ParseError> {
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let file_type = FileType::from_extension(&extension);

        // Select appropriate parser
        let parser: &dyn ContentParser = match file_type {
            FileType::TextDocument => &self.text_parser,
            FileType::Pdf => &self.pdf_parser,
            FileType::Code => &self.code_parser,
            _ => {
                // Try text parser as fallback for unknown types
                if self.text_parser.supported_extensions().contains(&extension.as_str()) {
                    &self.text_parser
                } else if self.code_parser.supported_extensions().contains(&extension.as_str()) {
                    &self.code_parser
                } else {
                    return Err(ParseError::UnsupportedFileType { extension });
                }
            }
        };

        parser.parse(path, &self.config).await
    }

    /// Parse with custom config
    pub async fn parse_with_config(
        &self,
        path: &Path,
        config: &ParseConfig,
    ) -> Result<ParseResult, ParseError> {
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let file_type = FileType::from_extension(&extension);

        let parser: &dyn ContentParser = match file_type {
            FileType::TextDocument => &self.text_parser,
            FileType::Pdf => &self.pdf_parser,
            FileType::Code => &self.code_parser,
            _ => {
                if self.text_parser.supported_extensions().contains(&extension.as_str()) {
                    &self.text_parser
                } else if self.code_parser.supported_extensions().contains(&extension.as_str()) {
                    &self.code_parser
                } else {
                    return Err(ParseError::UnsupportedFileType { extension });
                }
            }
        };

        parser.parse(path, config).await
    }

    /// Check if a file type is supported
    pub fn is_supported(&self, path: &Path) -> bool {
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        self.text_parser.supported_extensions().contains(&extension.as_str())
            || self.pdf_parser.supported_extensions().contains(&extension.as_str())
            || self.code_parser.supported_extensions().contains(&extension.as_str())
    }
}

impl Default for ContentParserService {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to create chunks from text
pub fn create_chunks_from_text(
    file_id: Uuid,
    text: &str,
    config: &ParseConfig,
    chunk_type: ChunkType,
) -> Vec<ContentChunk> {
    let mut chunks = Vec::new();
    let text_len = text.len();

    if text_len == 0 {
        return chunks;
    }

    // For small texts, create a single chunk
    if text_len <= config.max_chunk_size {
        let line_count = text.lines().count();
        chunks.push(ContentChunk {
            id: Uuid::now_v7(),
            file_id,
            chunk_index: 0,
            chunk_type,
            content: text.to_string(),
            location: ChunkLocation {
                start_offset: 0,
                end_offset: text_len as u64,
                start_line: Some(1),
                end_line: Some(line_count as u32),
                page_number: None,
                bounding_box: None,
            },
            vector_id: 0,
            created_at: Utc::now(),
        });
        return chunks;
    }

    // Split into chunks with overlap
    let mut start = 0;
    let mut chunk_index = 0;
    let mut current_line = 1u32;

    while start < text_len {
        let end = (start + config.max_chunk_size).min(text_len);

        // Try to find a good break point (paragraph or sentence boundary)
        let actual_end = find_break_point(text, start, end, config.min_chunk_size);

        let chunk_text = &text[start..actual_end];
        let chunk_lines = chunk_text.lines().count();

        chunks.push(ContentChunk {
            id: Uuid::now_v7(),
            file_id,
            chunk_index,
            chunk_type,
            content: chunk_text.to_string(),
            location: ChunkLocation {
                start_offset: start as u64,
                end_offset: actual_end as u64,
                start_line: Some(current_line),
                end_line: Some(current_line + chunk_lines.saturating_sub(1) as u32),
                page_number: None,
                bounding_box: None,
            },
            vector_id: 0,
            created_at: Utc::now(),
        });

        current_line += chunk_lines as u32;
        chunk_index += 1;

        // Move start with overlap
        if actual_end >= text_len {
            break;
        }
        start = actual_end.saturating_sub(config.chunk_overlap);
    }

    chunks
}

/// Find a good break point for chunking (paragraph or sentence boundary)
fn find_break_point(text: &str, start: usize, max_end: usize, min_size: usize) -> usize {
    let search_text = &text[start..max_end];

    // Try to find paragraph break (double newline)
    if let Some(pos) = search_text.rfind("\n\n") {
        let break_pos = start + pos + 2;
        if break_pos - start >= min_size {
            return break_pos;
        }
    }

    // Try to find sentence break
    for pattern in &[". ", "。", "! ", "? ", "！", "？"] {
        if let Some(pos) = search_text.rfind(pattern) {
            let break_pos = start + pos + pattern.len();
            if break_pos - start >= min_size {
                return break_pos;
            }
        }
    }

    // Try to find line break
    if let Some(pos) = search_text.rfind('\n') {
        let break_pos = start + pos + 1;
        if break_pos - start >= min_size {
            return break_pos;
        }
    }

    // Fall back to max_end
    max_end
}
