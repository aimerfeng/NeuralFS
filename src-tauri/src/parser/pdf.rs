//! PDF file parser
//!
//! Handles parsing of PDF documents with page-level text extraction.

use super::{ContentParser, ParseConfig, ParseError, ParseMetadata, ParseResult};
use crate::core::types::chunk::{ChunkLocation, ChunkType, ContentChunk};
use crate::core::types::file::FileType;
use async_trait::async_trait;
use chrono::Utc;
use std::path::Path;
use uuid::Uuid;

/// Parser for PDF documents
pub struct PdfParser {
    supported_extensions: Vec<&'static str>,
}

impl PdfParser {
    /// Create a new PDF parser
    pub fn new() -> Self {
        Self {
            supported_extensions: vec!["pdf"],
        }
    }

    /// Extract text from PDF using pdf-extract crate
    fn extract_pdf_text(&self, path: &Path) -> Result<PdfContent, ParseError> {
        let bytes = std::fs::read(path).map_err(|e| ParseError::Io(e))?;

        // Use pdf-extract to get text
        let text = pdf_extract::extract_text_from_mem(&bytes).map_err(|e| {
            ParseError::ParseFailed {
                reason: format!("PDF extraction failed: {}", e),
            }
        })?;

        // Try to get page count (pdf-extract doesn't provide this directly,
        // so we estimate based on form feeds or use a default)
        let page_count = self.estimate_page_count(&text);

        Ok(PdfContent {
            text,
            page_count,
            title: None,
            author: None,
        })
    }

    /// Estimate page count from extracted text
    /// PDF text extraction often includes form feed characters between pages
    fn estimate_page_count(&self, text: &str) -> u32 {
        // Count form feed characters (common page separator)
        let ff_count = text.matches('\x0C').count();
        if ff_count > 0 {
            return (ff_count + 1) as u32;
        }

        // Fallback: estimate based on text length (rough approximation)
        // Average page has ~3000 characters
        let char_count = text.chars().count();
        ((char_count / 3000) + 1).max(1) as u32
    }

    /// Split PDF text into page-based chunks
    fn create_page_chunks(
        &self,
        file_id: Uuid,
        text: &str,
        page_count: u32,
        config: &ParseConfig,
    ) -> Vec<ContentChunk> {
        let mut chunks = Vec::new();

        // Try to split by form feed characters first
        let pages: Vec<&str> = text.split('\x0C').collect();

        if pages.len() > 1 {
            // We have page separators
            for (page_num, page_text) in pages.iter().enumerate() {
                let page_text = page_text.trim();
                if page_text.is_empty() {
                    continue;
                }

                // If page is too large, split it further
                if page_text.len() > config.max_chunk_size {
                    let sub_chunks = self.split_page_into_chunks(
                        file_id,
                        page_text,
                        (page_num + 1) as u32,
                        chunks.len() as u32,
                        config,
                    );
                    chunks.extend(sub_chunks);
                } else {
                    chunks.push(ContentChunk {
                        id: Uuid::now_v7(),
                        file_id,
                        chunk_index: chunks.len() as u32,
                        chunk_type: ChunkType::Paragraph,
                        content: page_text.to_string(),
                        location: ChunkLocation {
                            start_offset: 0,
                            end_offset: page_text.len() as u64,
                            start_line: None,
                            end_line: None,
                            page_number: Some((page_num + 1) as u32),
                            bounding_box: None,
                        },
                        vector_id: 0,
                        created_at: Utc::now(),
                    });
                }
            }
        } else {
            // No page separators, split by size and estimate pages
            let chars_per_page = text.len() / page_count as usize;
            let mut current_offset = 0usize;
            let mut chunk_index = 0u32;

            while current_offset < text.len() {
                let end = (current_offset + config.max_chunk_size).min(text.len());
                let actual_end = find_paragraph_break(text, current_offset, end, config.min_chunk_size);

                let chunk_text = &text[current_offset..actual_end];
                let estimated_page = ((current_offset / chars_per_page.max(1)) + 1) as u32;

                chunks.push(ContentChunk {
                    id: Uuid::now_v7(),
                    file_id,
                    chunk_index,
                    chunk_type: ChunkType::Paragraph,
                    content: chunk_text.to_string(),
                    location: ChunkLocation {
                        start_offset: current_offset as u64,
                        end_offset: actual_end as u64,
                        start_line: None,
                        end_line: None,
                        page_number: Some(estimated_page.min(page_count)),
                        bounding_box: None,
                    },
                    vector_id: 0,
                    created_at: Utc::now(),
                });

                chunk_index += 1;
                if actual_end >= text.len() {
                    break;
                }
                current_offset = actual_end.saturating_sub(config.chunk_overlap);
            }
        }

        chunks
    }

    /// Split a single page into smaller chunks
    fn split_page_into_chunks(
        &self,
        file_id: Uuid,
        page_text: &str,
        page_number: u32,
        start_chunk_index: u32,
        config: &ParseConfig,
    ) -> Vec<ContentChunk> {
        let mut chunks = Vec::new();
        let mut current_offset = 0usize;
        let mut chunk_index = start_chunk_index;

        while current_offset < page_text.len() {
            let end = (current_offset + config.max_chunk_size).min(page_text.len());
            let actual_end = find_paragraph_break(page_text, current_offset, end, config.min_chunk_size);

            let chunk_text = &page_text[current_offset..actual_end];

            chunks.push(ContentChunk {
                id: Uuid::now_v7(),
                file_id,
                chunk_index,
                chunk_type: ChunkType::Paragraph,
                content: chunk_text.to_string(),
                location: ChunkLocation {
                    start_offset: current_offset as u64,
                    end_offset: actual_end as u64,
                    start_line: None,
                    end_line: None,
                    page_number: Some(page_number),
                    bounding_box: None,
                },
                vector_id: 0,
                created_at: Utc::now(),
            });

            chunk_index += 1;
            if actual_end >= page_text.len() {
                break;
            }
            current_offset = actual_end.saturating_sub(config.chunk_overlap);
        }

        chunks
    }
}

impl Default for PdfParser {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ContentParser for PdfParser {
    async fn parse(&self, path: &Path, config: &ParseConfig) -> Result<ParseResult, ParseError> {
        // Check if file exists
        if !path.exists() {
            return Err(ParseError::FileNotFound {
                path: path.display().to_string(),
            });
        }

        // Extract PDF content (blocking operation, run in spawn_blocking)
        let path_owned = path.to_path_buf();
        let parser = Self::new();
        let pdf_content = tokio::task::spawn_blocking(move || {
            parser.extract_pdf_text(&path_owned)
        })
        .await
        .map_err(|e| ParseError::ParseFailed {
            reason: format!("Task join error: {}", e),
        })??;

        let file_id = Uuid::now_v7();
        let chunks = self.create_page_chunks(
            file_id,
            &pdf_content.text,
            pdf_content.page_count,
            config,
        );

        let metadata = ParseMetadata {
            title: pdf_content.title,
            author: pdf_content.author,
            page_count: Some(pdf_content.page_count),
            word_count: pdf_content.text.split_whitespace().count(),
            char_count: pdf_content.text.chars().count(),
            ..Default::default()
        };

        Ok(ParseResult {
            text: pdf_content.text,
            chunks,
            metadata,
        })
    }

    fn supports(&self, file_type: FileType) -> bool {
        matches!(file_type, FileType::Pdf)
    }

    fn supported_extensions(&self) -> &[&str] {
        &self.supported_extensions
    }
}

/// Extracted PDF content
struct PdfContent {
    text: String,
    page_count: u32,
    title: Option<String>,
    author: Option<String>,
}

/// Find a good paragraph break point
fn find_paragraph_break(text: &str, start: usize, max_end: usize, min_size: usize) -> usize {
    let search_text = &text[start..max_end];

    // Try to find paragraph break (double newline)
    if let Some(pos) = search_text.rfind("\n\n") {
        let break_pos = start + pos + 2;
        if break_pos - start >= min_size {
            return break_pos;
        }
    }

    // Try to find sentence break
    for pattern in &[". ", "。", "! ", "? "] {
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

    max_end
}
