//! Text Preview Generator
//!
//! Generates text previews with snippet extraction and highlighting.
//! Supports text files, markdown, code files, and other text-based formats.

use super::{PreviewConfig, PreviewError};
use crate::core::types::chunk::ChunkLocation;
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

/// Text preview generator
pub struct TextPreviewGenerator {
    config: PreviewConfig,
}

impl TextPreviewGenerator {
    /// Create a new text preview generator
    pub fn new(config: PreviewConfig) -> Self {
        Self { config }
    }

    /// Generate a text preview from a file
    pub async fn generate(
        &self,
        path: &Path,
        file_id: Uuid,
        _location: Option<&ChunkLocation>,
    ) -> Result<TextPreview, PreviewError> {
        if !path.exists() {
            return Err(PreviewError::FileNotFound {
                path: path.display().to_string(),
            });
        }

        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::InvalidData {
                PreviewError::GenerationFailed {
                    reason: "File is not valid UTF-8".to_string(),
                }
            } else {
                PreviewError::Io(e)
            }
        })?;

        let snippet = self.extract_snippet(&content, 0, self.config.max_snippet_length);
        let line_count = content.lines().count();
        let char_count = content.chars().count();

        Ok(TextPreview {
            file_id,
            snippet,
            highlights: Vec::new(),
            total_lines: line_count,
            total_chars: char_count,
            start_line: 1,
            end_line: content[..self.config.max_snippet_length.min(content.len())]
                .lines()
                .count(),
            has_more: char_count > self.config.max_snippet_length,
        })
    }

    /// Generate a text preview with highlighting at a specific location
    pub async fn generate_with_highlight(
        &self,
        path: &Path,
        file_id: Uuid,
        location: &ChunkLocation,
        query: Option<&str>,
    ) -> Result<TextPreview, PreviewError> {
        if !path.exists() {
            return Err(PreviewError::FileNotFound {
                path: path.display().to_string(),
            });
        }

        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::InvalidData {
                PreviewError::GenerationFailed {
                    reason: "File is not valid UTF-8".to_string(),
                }
            } else {
                PreviewError::Io(e)
            }
        })?;

        let line_count = content.lines().count();
        let char_count = content.chars().count();

        // Calculate the snippet range based on location
        let (snippet, start_line, end_line, highlights) = if let Some(start) = location.start_line {
            self.extract_snippet_with_context(&content, start as usize, location, query)
        } else {
            // Fall back to byte offset
            let start_offset = location.start_offset as usize;
            let snippet = self.extract_snippet(&content, start_offset, self.config.max_snippet_length);
            let start_line = content[..start_offset.min(content.len())]
                .lines()
                .count()
                .max(1);
            let end_line = start_line + snippet.lines().count();
            let highlights = query
                .map(|q| self.find_highlights(&snippet, q))
                .unwrap_or_default();
            (snippet, start_line, end_line, highlights)
        };

        Ok(TextPreview {
            file_id,
            snippet,
            highlights,
            total_lines: line_count,
            total_chars: char_count,
            start_line,
            end_line,
            has_more: char_count > self.config.max_snippet_length,
        })
    }

    /// Extract a snippet from content starting at a byte offset
    fn extract_snippet(&self, content: &str, start_offset: usize, max_length: usize) -> String {
        let start = start_offset.min(content.len());
        let end = (start + max_length).min(content.len());

        // Adjust start to line boundary
        let adjusted_start = if start > 0 {
            content[..start]
                .rfind('\n')
                .map(|pos| pos + 1)
                .unwrap_or(0)
        } else {
            0
        };

        // Adjust end to line boundary
        let adjusted_end = content[end..]
            .find('\n')
            .map(|pos| end + pos)
            .unwrap_or(content.len());

        content[adjusted_start..adjusted_end].to_string()
    }

    /// Extract a snippet with context lines around a specific line
    fn extract_snippet_with_context(
        &self,
        content: &str,
        target_line: usize,
        location: &ChunkLocation,
        query: Option<&str>,
    ) -> (String, usize, usize, Vec<HighlightRange>) {
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        if total_lines == 0 {
            return (String::new(), 1, 1, Vec::new());
        }

        // Calculate start and end lines with context
        let target_idx = (target_line.saturating_sub(1)).min(total_lines - 1);
        let context = self.config.context_lines;

        let start_idx = target_idx.saturating_sub(context);
        let end_idx = if let Some(end_line) = location.end_line {
            ((end_line as usize).saturating_sub(1) + context).min(total_lines - 1)
        } else {
            (target_idx + context).min(total_lines - 1)
        };

        // Build snippet
        let snippet_lines: Vec<&str> = lines[start_idx..=end_idx].to_vec();
        let snippet = snippet_lines.join("\n");

        // Find highlights in the snippet
        let highlights = query
            .map(|q| self.find_highlights(&snippet, q))
            .unwrap_or_default();

        (snippet, start_idx + 1, end_idx + 1, highlights)
    }

    /// Find all occurrences of a query string in the snippet
    fn find_highlights(&self, snippet: &str, query: &str) -> Vec<HighlightRange> {
        if query.is_empty() {
            return Vec::new();
        }

        let query_lower = query.to_lowercase();
        let snippet_lower = snippet.to_lowercase();

        let mut highlights = Vec::new();
        let mut search_start = 0;

        while let Some(pos) = snippet_lower[search_start..].find(&query_lower) {
            let absolute_pos = search_start + pos;
            highlights.push(HighlightRange {
                start: absolute_pos as u32,
                end: (absolute_pos + query.len()) as u32,
                highlight_type: HighlightType::QueryMatch,
            });
            search_start = absolute_pos + 1;
        }

        highlights
    }
}

/// Text preview result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextPreview {
    /// File UUID
    pub file_id: Uuid,
    /// Extracted text snippet
    pub snippet: String,
    /// Highlight ranges within the snippet
    pub highlights: Vec<HighlightRange>,
    /// Total lines in the file
    pub total_lines: usize,
    /// Total characters in the file
    pub total_chars: usize,
    /// Starting line number of the snippet
    pub start_line: usize,
    /// Ending line number of the snippet
    pub end_line: usize,
    /// Whether there is more content beyond the snippet
    pub has_more: bool,
}

/// A range to highlight in the preview
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HighlightRange {
    /// Start position (character offset within snippet)
    pub start: u32,
    /// End position (character offset within snippet)
    pub end: u32,
    /// Type of highlight
    pub highlight_type: HighlightType,
}

/// Type of highlight
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum HighlightType {
    /// Match from search query
    QueryMatch,
    /// Semantic match
    SemanticMatch,
    /// Keyword match
    KeywordMatch,
    /// User-defined highlight
    Custom,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[tokio::test]
    async fn test_generate_text_preview() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "Line 1: Hello World").unwrap();
        writeln!(file, "Line 2: This is a test").unwrap();
        writeln!(file, "Line 3: More content here").unwrap();

        let config = PreviewConfig::default();
        let generator = TextPreviewGenerator::new(config);
        let file_id = Uuid::new_v4();

        let preview = generator
            .generate(file.path(), file_id, None)
            .await
            .unwrap();

        assert_eq!(preview.file_id, file_id);
        assert!(preview.snippet.contains("Hello World"));
        assert_eq!(preview.total_lines, 3);
        assert_eq!(preview.start_line, 1);
    }

    #[tokio::test]
    async fn test_generate_with_highlight() {
        let mut file = NamedTempFile::new().unwrap();
        for i in 1..=20 {
            writeln!(file, "Line {}: Content for line {}", i, i).unwrap();
        }

        let config = PreviewConfig {
            context_lines: 2,
            ..Default::default()
        };
        let generator = TextPreviewGenerator::new(config);
        let file_id = Uuid::new_v4();

        let location = ChunkLocation {
            start_offset: 0,
            end_offset: 100,
            start_line: Some(10),
            end_line: Some(12),
            page_number: None,
            bounding_box: None,
        };

        let preview = generator
            .generate_with_highlight(file.path(), file_id, &location, Some("Content"))
            .await
            .unwrap();

        assert!(preview.snippet.contains("Line 10"));
        assert!(!preview.highlights.is_empty());
        // Should include context lines (8-14 approximately)
        assert!(preview.start_line <= 10);
        assert!(preview.end_line >= 12);
    }

    #[test]
    fn test_find_highlights() {
        let config = PreviewConfig::default();
        let generator = TextPreviewGenerator::new(config);

        let snippet = "Hello World, hello again, HELLO!";
        let highlights = generator.find_highlights(snippet, "hello");

        assert_eq!(highlights.len(), 3);
        assert_eq!(highlights[0].start, 0);
        assert_eq!(highlights[0].end, 5);
    }

    #[test]
    fn test_extract_snippet() {
        let config = PreviewConfig {
            max_snippet_length: 50,
            ..Default::default()
        };
        let generator = TextPreviewGenerator::new(config);

        let content = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5";
        let snippet = generator.extract_snippet(content, 0, 20);

        assert!(snippet.starts_with("Line 1"));
    }
}
