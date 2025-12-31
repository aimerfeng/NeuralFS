//! File Preview Generation Module
//!
//! Provides preview generation for various file types:
//! - Text files with snippet extraction and highlighting
//! - Images with scaling and region marking
//! - Documents (PDF) with page rendering and paragraph location

mod text;
mod image;
mod document;
#[cfg(test)]
mod tests;

pub use text::{TextPreviewGenerator, TextPreview, HighlightRange};
pub use image::{ImagePreviewGenerator, ImagePreview, RegionMarker};
pub use document::{DocumentPreviewGenerator, DocumentPreview, PagePreview};

use crate::core::types::chunk::ChunkLocation;
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur during preview generation
#[derive(Error, Debug)]
pub enum PreviewError {
    #[error("File not found: {path}")]
    FileNotFound { path: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Unsupported file type: {extension}")]
    UnsupportedFileType { extension: String },

    #[error("Preview generation failed: {reason}")]
    GenerationFailed { reason: String },

    #[error("Image processing error: {reason}")]
    ImageError { reason: String },

    #[error("Document processing error: {reason}")]
    DocumentError { reason: String },
}

/// Configuration for preview generation
#[derive(Debug, Clone)]
pub struct PreviewConfig {
    /// Maximum snippet length for text previews (characters)
    pub max_snippet_length: usize,
    /// Context lines before/after highlight
    pub context_lines: usize,
    /// Maximum image preview width
    pub max_image_width: u32,
    /// Maximum image preview height
    pub max_image_height: u32,
    /// JPEG quality for image previews (1-100)
    pub jpeg_quality: u8,
    /// Maximum PDF pages to render for preview
    pub max_pdf_pages: usize,
}

impl Default for PreviewConfig {
    fn default() -> Self {
        Self {
            max_snippet_length: 500,
            context_lines: 3,
            max_image_width: 800,
            max_image_height: 600,
            jpeg_quality: 85,
            max_pdf_pages: 5,
        }
    }
}

/// Main preview service that delegates to type-specific generators
pub struct PreviewService {
    text_generator: TextPreviewGenerator,
    image_generator: ImagePreviewGenerator,
    document_generator: DocumentPreviewGenerator,
    config: PreviewConfig,
}

impl PreviewService {
    /// Create a new preview service with default config
    pub fn new() -> Self {
        Self::with_config(PreviewConfig::default())
    }

    /// Create a new preview service with custom config
    pub fn with_config(config: PreviewConfig) -> Self {
        Self {
            text_generator: TextPreviewGenerator::new(config.clone()),
            image_generator: ImagePreviewGenerator::new(config.clone()),
            document_generator: DocumentPreviewGenerator::new(config.clone()),
            config,
        }
    }

    /// Generate a preview for a file
    pub async fn generate_preview(
        &self,
        path: &Path,
        file_id: Uuid,
    ) -> Result<GeneratedPreview, PreviewError> {
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match extension.as_str() {
            // Text files
            "txt" | "md" | "markdown" | "rst" | "json" | "yaml" | "yml" 
            | "toml" | "xml" | "csv" | "log" | "ini" | "cfg" | "conf" => {
                let preview = self.text_generator.generate(path, file_id, None).await?;
                Ok(GeneratedPreview::Text(preview))
            }
            // Code files
            "rs" | "py" | "js" | "ts" | "jsx" | "tsx" | "java" | "c" | "cpp" 
            | "h" | "hpp" | "go" | "rb" | "php" | "swift" | "kt" | "scala"
            | "html" | "css" | "scss" | "less" | "sql" | "sh" | "bash" | "ps1" => {
                let preview = self.text_generator.generate(path, file_id, None).await?;
                Ok(GeneratedPreview::Text(preview))
            }
            // Image files
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "tiff" => {
                let preview = self.image_generator.generate(path, file_id, None).await?;
                Ok(GeneratedPreview::Image(preview))
            }
            // PDF documents
            "pdf" => {
                let preview = self.document_generator.generate(path, file_id, None).await?;
                Ok(GeneratedPreview::Document(preview))
            }
            _ => Err(PreviewError::UnsupportedFileType { extension }),
        }
    }

    /// Generate a preview with highlight at specific location
    pub async fn generate_preview_with_highlight(
        &self,
        path: &Path,
        file_id: Uuid,
        location: &ChunkLocation,
        query: Option<&str>,
    ) -> Result<GeneratedPreview, PreviewError> {
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match extension.as_str() {
            // Text and code files
            "txt" | "md" | "markdown" | "rst" | "json" | "yaml" | "yml" 
            | "toml" | "xml" | "csv" | "log" | "ini" | "cfg" | "conf"
            | "rs" | "py" | "js" | "ts" | "jsx" | "tsx" | "java" | "c" | "cpp" 
            | "h" | "hpp" | "go" | "rb" | "php" | "swift" | "kt" | "scala"
            | "html" | "css" | "scss" | "less" | "sql" | "sh" | "bash" | "ps1" => {
                let preview = self.text_generator
                    .generate_with_highlight(path, file_id, location, query)
                    .await?;
                Ok(GeneratedPreview::Text(preview))
            }
            // Image files
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "tiff" => {
                let preview = self.image_generator
                    .generate(path, file_id, location.bounding_box)
                    .await?;
                Ok(GeneratedPreview::Image(preview))
            }
            // PDF documents
            "pdf" => {
                let preview = self.document_generator
                    .generate(path, file_id, location.page_number)
                    .await?;
                Ok(GeneratedPreview::Document(preview))
            }
            _ => Err(PreviewError::UnsupportedFileType { extension }),
        }
    }

    /// Get the configuration
    pub fn config(&self) -> &PreviewConfig {
        &self.config
    }
}

impl Default for PreviewService {
    fn default() -> Self {
        Self::new()
    }
}

/// Generated preview result
#[derive(Debug, Clone)]
pub enum GeneratedPreview {
    Text(TextPreview),
    Image(ImagePreview),
    Document(DocumentPreview),
}

impl GeneratedPreview {
    /// Get the content type for HTTP response
    pub fn content_type(&self) -> &str {
        match self {
            GeneratedPreview::Text(_) => "application/json",
            GeneratedPreview::Image(img) => &img.content_type,
            GeneratedPreview::Document(_) => "application/json",
        }
    }

    /// Serialize to bytes for caching
    pub fn to_bytes(&self) -> Result<Vec<u8>, PreviewError> {
        match self {
            GeneratedPreview::Text(preview) => {
                serde_json::to_vec(preview).map_err(|e| PreviewError::GenerationFailed {
                    reason: format!("JSON serialization failed: {}", e),
                })
            }
            GeneratedPreview::Image(preview) => Ok(preview.data.clone()),
            GeneratedPreview::Document(preview) => {
                serde_json::to_vec(preview).map_err(|e| PreviewError::GenerationFailed {
                    reason: format!("JSON serialization failed: {}", e),
                })
            }
        }
    }
}
