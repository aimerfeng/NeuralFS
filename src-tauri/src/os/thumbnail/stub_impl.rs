//! Stub thumbnail extractor for non-Windows platforms
//!
//! This provides a fallback implementation that returns an error
//! indicating the platform is not supported for native thumbnail extraction.

use std::path::Path;

use super::{ImageFormat, ThumbnailData, ThumbnailExtractorTrait, ThumbnailSize};
use crate::core::error::{OsError, Result};

/// Stub thumbnail extractor for non-Windows platforms
pub struct StubThumbnailExtractor;

impl StubThumbnailExtractor {
    /// Create a new stub thumbnail extractor
    pub fn new() -> Result<Self> {
        Ok(Self)
    }
}

impl Default for StubThumbnailExtractor {
    fn default() -> Self {
        Self
    }
}

impl ThumbnailExtractorTrait for StubThumbnailExtractor {
    fn get_thumbnail(&self, path: &Path, _size: ThumbnailSize) -> Result<ThumbnailData> {
        // On non-Windows platforms, we could potentially use:
        // - macOS: QLThumbnailGenerator
        // - Linux: Freedesktop thumbnail spec or GdkPixbuf
        //
        // For now, return an error indicating the platform is not supported
        Err(OsError::PlatformNotSupported {
            platform: format!(
                "Thumbnail extraction not implemented for this platform. Path: {}",
                path.display()
            ),
        }
        .into())
    }

    fn is_supported(&self, _path: &Path) -> bool {
        // Stub implementation doesn't support any files
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_stub_extractor_creation() {
        let extractor = StubThumbnailExtractor::new();
        assert!(extractor.is_ok());
    }

    #[test]
    fn test_stub_is_not_supported() {
        let extractor = StubThumbnailExtractor::default();
        assert!(!extractor.is_supported(&PathBuf::from("test.jpg")));
        assert!(!extractor.is_supported(&PathBuf::from("test.png")));
    }

    #[test]
    fn test_stub_get_thumbnail_returns_error() {
        let extractor = StubThumbnailExtractor::default();
        let result = extractor.get_thumbnail(&PathBuf::from("test.jpg"), ThumbnailSize::Medium);
        assert!(result.is_err());
    }
}
