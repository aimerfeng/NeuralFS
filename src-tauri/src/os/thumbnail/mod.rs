//! System Thumbnail Extraction
//!
//! This module provides cross-platform thumbnail extraction using native OS APIs:
//! - Windows: IShellItemImageFactory
//! - macOS: QLThumbnailGenerator (stub)
//! - Linux: Freedesktop thumbnail spec (stub)
//!
//! Also includes a two-tier caching system:
//! - Memory cache: Fast LRU cache for frequently accessed thumbnails
//! - Disk cache: Persistent storage for thumbnails across sessions

use std::path::Path;
use crate::core::error::Result;

mod cache;

pub use cache::{
    CacheEntryMetadata, CacheKey, CacheStats, ThumbnailCache, ThumbnailCacheConfig,
};

/// Thumbnail size presets
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailSize {
    /// 48x48 pixels
    Small,
    /// 96x96 pixels
    Medium,
    /// 256x256 pixels
    Large,
    /// 512x512 pixels
    XLarge,
}

impl ThumbnailSize {
    /// Get the dimensions (width, height) for this size
    pub fn dimensions(&self) -> (i32, i32) {
        match self {
            Self::Small => (48, 48),
            Self::Medium => (96, 96),
            Self::Large => (256, 256),
            Self::XLarge => (512, 512),
        }
    }

    /// Get width
    pub fn width(&self) -> i32 {
        self.dimensions().0
    }

    /// Get height
    pub fn height(&self) -> i32 {
        self.dimensions().1
    }
}

impl Default for ThumbnailSize {
    fn default() -> Self {
        Self::Medium
    }
}

/// Image format for thumbnail data
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Bmp,
}

impl ImageFormat {
    /// Get MIME type for this format
    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Bmp => "image/bmp",
        }
    }

    /// Get file extension for this format
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Bmp => "bmp",
        }
    }
}

/// Thumbnail data returned by the extractor
#[derive(Debug, Clone)]
pub struct ThumbnailData {
    /// Raw image data
    pub data: Vec<u8>,
    /// Image format
    pub format: ImageFormat,
    /// Actual width of the thumbnail
    pub width: u32,
    /// Actual height of the thumbnail
    pub height: u32,
}

impl ThumbnailData {
    /// Create new thumbnail data
    pub fn new(data: Vec<u8>, format: ImageFormat, width: u32, height: u32) -> Self {
        Self {
            data,
            format,
            width,
            height,
        }
    }

    /// Get the MIME type
    pub fn mime_type(&self) -> &'static str {
        self.format.mime_type()
    }

    /// Check if the thumbnail data is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Get the size of the thumbnail data in bytes
    pub fn len(&self) -> usize {
        self.data.len()
    }
}

/// Thumbnail extractor trait for cross-platform abstraction
pub trait ThumbnailExtractorTrait: Send + Sync {
    /// Extract thumbnail from a file
    fn get_thumbnail(&self, path: &Path, size: ThumbnailSize) -> Result<ThumbnailData>;

    /// Check if thumbnail extraction is supported for this file type
    fn is_supported(&self, path: &Path) -> bool;
}

// Platform-specific implementations
#[cfg(windows)]
mod windows_impl;

#[cfg(windows)]
pub use windows_impl::WindowsThumbnailExtractor;

#[cfg(not(windows))]
mod stub_impl;

#[cfg(not(windows))]
pub use stub_impl::StubThumbnailExtractor;

/// Type alias for the platform-specific thumbnail extractor
#[cfg(windows)]
pub type ThumbnailExtractor = WindowsThumbnailExtractor;

#[cfg(not(windows))]
pub type ThumbnailExtractor = StubThumbnailExtractor;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thumbnail_size_dimensions() {
        assert_eq!(ThumbnailSize::Small.dimensions(), (48, 48));
        assert_eq!(ThumbnailSize::Medium.dimensions(), (96, 96));
        assert_eq!(ThumbnailSize::Large.dimensions(), (256, 256));
        assert_eq!(ThumbnailSize::XLarge.dimensions(), (512, 512));
    }

    #[test]
    fn test_thumbnail_size_default() {
        assert_eq!(ThumbnailSize::default(), ThumbnailSize::Medium);
    }

    #[test]
    fn test_image_format_mime_type() {
        assert_eq!(ImageFormat::Png.mime_type(), "image/png");
        assert_eq!(ImageFormat::Jpeg.mime_type(), "image/jpeg");
        assert_eq!(ImageFormat::Bmp.mime_type(), "image/bmp");
    }

    #[test]
    fn test_image_format_extension() {
        assert_eq!(ImageFormat::Png.extension(), "png");
        assert_eq!(ImageFormat::Jpeg.extension(), "jpg");
        assert_eq!(ImageFormat::Bmp.extension(), "bmp");
    }

    #[test]
    fn test_thumbnail_data_creation() {
        let data = vec![0u8; 100];
        let thumbnail = ThumbnailData::new(data.clone(), ImageFormat::Png, 96, 96);

        assert_eq!(thumbnail.len(), 100);
        assert!(!thumbnail.is_empty());
        assert_eq!(thumbnail.width, 96);
        assert_eq!(thumbnail.height, 96);
        assert_eq!(thumbnail.mime_type(), "image/png");
    }

    #[test]
    fn test_thumbnail_data_empty() {
        let thumbnail = ThumbnailData::new(vec![], ImageFormat::Png, 0, 0);
        assert!(thumbnail.is_empty());
        assert_eq!(thumbnail.len(), 0);
    }
}


/// Cached thumbnail extractor that combines extraction with caching
pub struct CachedThumbnailExtractor {
    /// The underlying thumbnail extractor
    extractor: ThumbnailExtractor,
    /// The thumbnail cache
    cache: ThumbnailCache,
}

impl CachedThumbnailExtractor {
    /// Create a new cached thumbnail extractor
    pub fn new(cache_dir: std::path::PathBuf) -> Result<Self> {
        let extractor = ThumbnailExtractor::new()?;
        let cache = ThumbnailCache::with_default_config(cache_dir)?;

        Ok(Self { extractor, cache })
    }

    /// Create with custom cache configuration
    pub fn with_config(config: ThumbnailCacheConfig) -> Result<Self> {
        let extractor = ThumbnailExtractor::new()?;
        let cache = ThumbnailCache::new(config)?;

        Ok(Self { extractor, cache })
    }

    /// Get a thumbnail, using cache if available
    pub fn get_thumbnail(&self, path: &Path, size: ThumbnailSize) -> Result<ThumbnailData> {
        // Try cache first
        if let Some(cached) = self.cache.get(path, size) {
            return Ok(cached);
        }

        // Extract thumbnail
        let thumbnail = self.extractor.get_thumbnail(path, size)?;

        // Cache the result
        let _ = self.cache.put(path, size, thumbnail.clone());

        Ok(thumbnail)
    }

    /// Check if thumbnail extraction is supported for this file
    pub fn is_supported(&self, path: &Path) -> bool {
        self.extractor.is_supported(path)
    }

    /// Invalidate cache for a file (all sizes)
    pub fn invalidate(&self, path: &Path) -> Result<()> {
        self.cache.remove_all_sizes(path)
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> CacheStats {
        self.cache.stats()
    }

    /// Clear all cached thumbnails
    pub fn clear_cache(&self) -> Result<()> {
        self.cache.clear()
    }

    /// Evict expired cache entries
    pub fn evict_expired(&self) -> Result<usize> {
        self.cache.evict_expired()
    }
}

#[cfg(test)]
mod cached_extractor_tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_cached_extractor_creation() {
        let temp_dir = TempDir::new().unwrap();
        let extractor = CachedThumbnailExtractor::new(temp_dir.path().to_path_buf());
        
        // On Windows, this should succeed
        // On other platforms, it depends on the stub implementation
        #[cfg(windows)]
        assert!(extractor.is_ok());
        
        #[cfg(not(windows))]
        assert!(extractor.is_ok());
    }

    #[test]
    fn test_cache_stats_initial() {
        let temp_dir = TempDir::new().unwrap();
        let extractor = CachedThumbnailExtractor::new(temp_dir.path().to_path_buf()).unwrap();
        
        let stats = extractor.cache_stats();
        assert_eq!(stats.memory_entries, 0);
        assert_eq!(stats.disk_size_bytes, 0);
    }
}
