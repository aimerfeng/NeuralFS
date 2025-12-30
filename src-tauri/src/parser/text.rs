//! Text file parser
//!
//! Handles parsing of plain text files including TXT, MD, and JSON.

use super::{
    create_chunks_from_text, ContentParser, ParseConfig, ParseError, ParseMetadata, ParseResult,
};
use crate::core::types::chunk::ChunkType;
use crate::core::types::file::FileType;
use async_trait::async_trait;
use std::path::Path;
use uuid::Uuid;

/// Parser for text-based files (TXT, MD, JSON)
pub struct TextParser {
    supported_extensions: Vec<&'static str>,
}

impl TextParser {
    /// Create a new text parser
    pub fn new() -> Self {
        Self {
            supported_extensions: vec![
                "txt", "md", "markdown", "rst", "rtf", "json", "yaml", "yml", "toml", "xml",
                "csv", "log", "ini", "cfg", "conf",
            ],
        }
    }

    /// Parse plain text content
    fn parse_plain_text(
        &self,
        content: &str,
        file_id: Uuid,
        config: &ParseConfig,
    ) -> ParseResult {
        let chunks = create_chunks_from_text(file_id, content, config, ChunkType::Paragraph);

        let metadata = ParseMetadata {
            word_count: content.split_whitespace().count(),
            char_count: content.chars().count(),
            ..Default::default()
        };

        ParseResult {
            text: content.to_string(),
            chunks,
            metadata,
        }
    }

    /// Parse Markdown content with structure awareness
    fn parse_markdown(
        &self,
        content: &str,
        file_id: Uuid,
        config: &ParseConfig,
    ) -> ParseResult {
        let mut chunks = Vec::new();
        let mut chunk_index = 0u32;
        let mut current_offset = 0usize;
        let mut current_line = 1u32;

        // Extract title from first heading
        let title = content
            .lines()
            .find(|line| line.starts_with("# "))
            .map(|line| line.trim_start_matches("# ").to_string());

        // Split by headings to preserve document structure
        let sections = split_markdown_sections(content);

        for section in sections {
            if section.content.trim().is_empty() {
                current_offset += section.content.len();
                current_line += section.content.lines().count() as u32;
                continue;
            }

            let chunk_type = if section.is_heading {
                ChunkType::Heading
            } else if section.is_code_block {
                ChunkType::CodeBlock
            } else {
                ChunkType::Paragraph
            };

            // If section is too large, split it further
            if section.content.len() > config.max_chunk_size && !section.is_code_block {
                let sub_chunks = create_chunks_from_text(
                    file_id,
                    &section.content,
                    config,
                    chunk_type,
                );
                for mut sub_chunk in sub_chunks {
                    sub_chunk.chunk_index = chunk_index;
                    // Adjust offsets
                    sub_chunk.location.start_offset += current_offset as u64;
                    sub_chunk.location.end_offset += current_offset as u64;
                    if let Some(ref mut start) = sub_chunk.location.start_line {
                        *start += current_line - 1;
                    }
                    if let Some(ref mut end) = sub_chunk.location.end_line {
                        *end += current_line - 1;
                    }
                    chunks.push(sub_chunk);
                    chunk_index += 1;
                }
            } else {
                let line_count = section.content.lines().count();
                chunks.push(crate::core::types::chunk::ContentChunk {
                    id: uuid::Uuid::now_v7(),
                    file_id,
                    chunk_index,
                    chunk_type,
                    content: section.content.clone(),
                    location: crate::core::types::chunk::ChunkLocation {
                        start_offset: current_offset as u64,
                        end_offset: (current_offset + section.content.len()) as u64,
                        start_line: Some(current_line),
                        end_line: Some(current_line + line_count.saturating_sub(1) as u32),
                        page_number: None,
                        bounding_box: None,
                    },
                    vector_id: 0,
                    created_at: chrono::Utc::now(),
                });
                chunk_index += 1;
            }

            current_offset += section.content.len();
            current_line += section.content.lines().count() as u32;
        }

        let metadata = ParseMetadata {
            title,
            word_count: content.split_whitespace().count(),
            char_count: content.chars().count(),
            ..Default::default()
        };

        ParseResult {
            text: content.to_string(),
            chunks,
            metadata,
        }
    }

    /// Parse JSON content
    fn parse_json(
        &self,
        content: &str,
        file_id: Uuid,
        config: &ParseConfig,
    ) -> Result<ParseResult, ParseError> {
        // Validate JSON
        let _: serde_json::Value = serde_json::from_str(content).map_err(|e| {
            ParseError::ParseFailed {
                reason: format!("Invalid JSON: {}", e),
            }
        })?;

        // For JSON, we treat the whole content as a single chunk or split by size
        let chunks = create_chunks_from_text(file_id, content, config, ChunkType::Paragraph);

        let metadata = ParseMetadata {
            word_count: content.split_whitespace().count(),
            char_count: content.chars().count(),
            ..Default::default()
        };

        Ok(ParseResult {
            text: content.to_string(),
            chunks,
            metadata,
        })
    }
}

impl Default for TextParser {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ContentParser for TextParser {
    async fn parse(&self, path: &Path, config: &ParseConfig) -> Result<ParseResult, ParseError> {
        // Check if file exists
        if !path.exists() {
            return Err(ParseError::FileNotFound {
                path: path.display().to_string(),
            });
        }

        // Read file content
        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::InvalidData {
                ParseError::EncodingError {
                    reason: "File is not valid UTF-8".to_string(),
                }
            } else {
                ParseError::Io(e)
            }
        })?;

        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        // Generate a file ID for chunks (in real usage, this would come from the database)
        let file_id = Uuid::now_v7();

        match extension.as_str() {
            "md" | "markdown" => Ok(self.parse_markdown(&content, file_id, config)),
            "json" => self.parse_json(&content, file_id, config),
            _ => Ok(self.parse_plain_text(&content, file_id, config)),
        }
    }

    fn supports(&self, file_type: FileType) -> bool {
        matches!(file_type, FileType::TextDocument)
    }

    fn supported_extensions(&self) -> &[&str] {
        &self.supported_extensions
    }
}

/// Section of a markdown document
struct MarkdownSection {
    content: String,
    is_heading: bool,
    is_code_block: bool,
}

/// Split markdown content into sections by headings and code blocks
fn split_markdown_sections(content: &str) -> Vec<MarkdownSection> {
    let mut sections = Vec::new();
    let mut current_section = String::new();
    let mut in_code_block = false;
    let mut is_heading = false;

    for line in content.lines() {
        // Check for code block markers
        if line.trim().starts_with("```") {
            if in_code_block {
                // End of code block
                current_section.push_str(line);
                current_section.push('\n');
                sections.push(MarkdownSection {
                    content: std::mem::take(&mut current_section),
                    is_heading: false,
                    is_code_block: true,
                });
                in_code_block = false;
            } else {
                // Start of code block - save current section first
                if !current_section.is_empty() {
                    sections.push(MarkdownSection {
                        content: std::mem::take(&mut current_section),
                        is_heading,
                        is_code_block: false,
                    });
                }
                current_section.push_str(line);
                current_section.push('\n');
                in_code_block = true;
                is_heading = false;
            }
            continue;
        }

        if in_code_block {
            current_section.push_str(line);
            current_section.push('\n');
            continue;
        }

        // Check for headings
        if line.starts_with('#') {
            // Save current section
            if !current_section.is_empty() {
                sections.push(MarkdownSection {
                    content: std::mem::take(&mut current_section),
                    is_heading,
                    is_code_block: false,
                });
            }
            is_heading = true;
        }

        current_section.push_str(line);
        current_section.push('\n');

        // After a heading line, mark next content as not heading
        if is_heading && !line.starts_with('#') {
            is_heading = false;
        }
    }

    // Don't forget the last section
    if !current_section.is_empty() {
        sections.push(MarkdownSection {
            content: current_section,
            is_heading,
            is_code_block: in_code_block,
        });
    }

    sections
}
