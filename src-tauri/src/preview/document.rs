//! Document Preview Generator
//!
//! Generates document previews with page rendering and paragraph location.
//! Currently supports PDF documents.

use super::{PreviewConfig, PreviewError};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

/// Document preview generator
pub struct DocumentPreviewGenerator {
    config: PreviewConfig,
}

impl DocumentPreviewGenerator {
    /// Create a new document preview generator
    pub fn new(config: PreviewConfig) -> Self {
        Self { config }
    }

    /// Generate a document preview from a file
    pub async fn generate(
        &self,
        path: &Path,
        file_id: Uuid,
        target_page: Option<u32>,
    ) -> Result<DocumentPreview, PreviewError> {
        if !path.exists() {
            return Err(PreviewError::FileNotFound {
                path: path.display().to_string(),
            });
        }

        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match extension.as_str() {
            "pdf" => self.generate_pdf_preview(path, file_id, target_page).await,
            _ => Err(PreviewError::UnsupportedFileType { extension }),
        }
    }

    /// Generate PDF preview
    async fn generate_pdf_preview(
        &self,
        path: &Path,
        file_id: Uuid,
        target_page: Option<u32>,
    ) -> Result<DocumentPreview, PreviewError> {
        let path_owned = path.to_path_buf();
        let max_pages = self.config.max_pdf_pages;
        let max_snippet_length = self.config.max_snippet_length;

        let result = tokio::task::spawn_blocking(move || {
            Self::extract_pdf_content(&path_owned, file_id, target_page, max_pages, max_snippet_length)
        })
        .await
        .map_err(|e| PreviewError::GenerationFailed {
            reason: format!("Task join error: {}", e),
        })??;

        Ok(result)
    }

    /// Extract PDF content synchronously
    fn extract_pdf_content(
        path: &Path,
        file_id: Uuid,
        target_page: Option<u32>,
        max_pages: usize,
        max_snippet_length: usize,
    ) -> Result<DocumentPreview, PreviewError> {
        let bytes = std::fs::read(path).map_err(|e| PreviewError::Io(e))?;

        // Extract text from PDF
        let full_text = pdf_extract::extract_text_from_mem(&bytes).map_err(|e| {
            PreviewError::DocumentError {
                reason: format!("PDF extraction failed: {}", e),
            }
        })?;

        // Split by form feed characters (page separators)
        let pages: Vec<&str> = full_text.split('\x0C').collect();
        let total_pages = pages.len().max(1) as u32;

        // Determine which pages to include in preview
        let (start_page, end_page) = if let Some(target) = target_page {
            // Center around target page
            let target_idx = (target as usize).saturating_sub(1).min(pages.len() - 1);
            let half_pages = max_pages / 2;
            let start = target_idx.saturating_sub(half_pages);
            let end = (start + max_pages).min(pages.len());
            (start, end)
        } else {
            // Start from beginning
            (0, max_pages.min(pages.len()))
        };

        // Build page previews
        let mut page_previews = Vec::new();
        for (idx, page_text) in pages.iter().enumerate().skip(start_page).take(end_page - start_page) {
            let page_number = (idx + 1) as u32;
            let text = page_text.trim();

            // Extract snippet for this page
            let snippet = if text.len() > max_snippet_length {
                let truncated = &text[..max_snippet_length];
                // Try to end at a sentence boundary
                if let Some(pos) = truncated.rfind(|c| c == '.' || c == '。' || c == '!' || c == '?') {
                    format!("{}...", &truncated[..=pos])
                } else {
                    format!("{}...", truncated)
                }
            } else {
                text.to_string()
            };

            // Extract paragraphs
            let paragraphs = Self::extract_paragraphs(text);

            page_previews.push(PagePreview {
                page_number,
                text: snippet,
                paragraphs,
                is_target: target_page.map(|t| t == page_number).unwrap_or(false),
            });
        }

        // Calculate word count
        let word_count = full_text.split_whitespace().count();

        Ok(DocumentPreview {
            file_id,
            document_type: DocumentType::Pdf,
            total_pages,
            pages: page_previews,
            target_page,
            word_count,
            has_more_pages: end_page < pages.len(),
        })
    }

    /// Extract paragraphs from page text
    fn extract_paragraphs(text: &str) -> Vec<ParagraphInfo> {
        let mut paragraphs = Vec::new();
        let mut current_offset = 0u32;

        // Split by double newlines (paragraph boundaries)
        for (idx, para_text) in text.split("\n\n").enumerate() {
            let trimmed = para_text.trim();
            if trimmed.is_empty() {
                current_offset += para_text.len() as u32 + 2; // +2 for \n\n
                continue;
            }

            let para_len = trimmed.len() as u32;
            paragraphs.push(ParagraphInfo {
                index: idx as u32,
                start_offset: current_offset,
                end_offset: current_offset + para_len,
                preview: if trimmed.len() > 100 {
                    format!("{}...", &trimmed[..100])
                } else {
                    trimmed.to_string()
                },
            });

            current_offset += para_text.len() as u32 + 2;
        }

        paragraphs
    }

    /// Locate a paragraph by offset within a page
    pub fn locate_paragraph(
        preview: &DocumentPreview,
        page_number: u32,
        offset: u32,
    ) -> Option<&ParagraphInfo> {
        preview
            .pages
            .iter()
            .find(|p| p.page_number == page_number)
            .and_then(|page| {
                page.paragraphs
                    .iter()
                    .find(|para| para.start_offset <= offset && offset < para.end_offset)
            })
    }
}

/// Document preview result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentPreview {
    /// File UUID
    pub file_id: Uuid,
    /// Document type
    pub document_type: DocumentType,
    /// Total number of pages
    pub total_pages: u32,
    /// Page previews
    pub pages: Vec<PagePreview>,
    /// Target page (if specified)
    pub target_page: Option<u32>,
    /// Total word count
    pub word_count: usize,
    /// Whether there are more pages beyond the preview
    pub has_more_pages: bool,
}

/// Document type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DocumentType {
    Pdf,
    Word,
    Presentation,
    Spreadsheet,
}

/// Preview of a single page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PagePreview {
    /// Page number (1-based)
    pub page_number: u32,
    /// Page text content (may be truncated)
    pub text: String,
    /// Paragraph information for navigation
    pub paragraphs: Vec<ParagraphInfo>,
    /// Whether this is the target page
    pub is_target: bool,
}

/// Information about a paragraph for navigation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParagraphInfo {
    /// Paragraph index within the page
    pub index: u32,
    /// Start offset within the page
    pub start_offset: u32,
    /// End offset within the page
    pub end_offset: u32,
    /// Preview text (first ~100 chars)
    pub preview: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_paragraphs() {
        let text = "First paragraph here.\n\nSecond paragraph with more content.\n\nThird paragraph.";
        let paragraphs = DocumentPreviewGenerator::extract_paragraphs(text);

        assert_eq!(paragraphs.len(), 3);
        assert_eq!(paragraphs[0].index, 0);
        assert!(paragraphs[0].preview.contains("First paragraph"));
        assert_eq!(paragraphs[1].index, 1);
        assert!(paragraphs[1].preview.contains("Second paragraph"));
    }

    #[test]
    fn test_extract_paragraphs_empty() {
        let text = "";
        let paragraphs = DocumentPreviewGenerator::extract_paragraphs(text);
        assert!(paragraphs.is_empty());
    }

    #[test]
    fn test_extract_paragraphs_single() {
        let text = "Single paragraph without breaks.";
        let paragraphs = DocumentPreviewGenerator::extract_paragraphs(text);
        assert_eq!(paragraphs.len(), 1);
        assert_eq!(paragraphs[0].preview, "Single paragraph without breaks.");
    }

    #[test]
    fn test_locate_paragraph() {
        let preview = DocumentPreview {
            file_id: Uuid::new_v4(),
            document_type: DocumentType::Pdf,
            total_pages: 1,
            pages: vec![PagePreview {
                page_number: 1,
                text: "Test".to_string(),
                paragraphs: vec![
                    ParagraphInfo {
                        index: 0,
                        start_offset: 0,
                        end_offset: 50,
                        preview: "First para".to_string(),
                    },
                    ParagraphInfo {
                        index: 1,
                        start_offset: 52,
                        end_offset: 100,
                        preview: "Second para".to_string(),
                    },
                ],
                is_target: false,
            }],
            target_page: None,
            word_count: 100,
            has_more_pages: false,
        };

        // Find paragraph at offset 25 (should be first paragraph)
        let para = DocumentPreviewGenerator::locate_paragraph(&preview, 1, 25);
        assert!(para.is_some());
        assert_eq!(para.unwrap().index, 0);

        // Find paragraph at offset 75 (should be second paragraph)
        let para = DocumentPreviewGenerator::locate_paragraph(&preview, 1, 75);
        assert!(para.is_some());
        assert_eq!(para.unwrap().index, 1);

        // Offset outside any paragraph
        let para = DocumentPreviewGenerator::locate_paragraph(&preview, 1, 51);
        assert!(para.is_none());
    }
}
