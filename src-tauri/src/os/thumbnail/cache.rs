//! Thumbnail Cache with LRU eviction and disk persistence
//!
//! This module provides a two-tier caching system for thumbnails:
//! - Memory cache: Fast LRU cache for frequently accessed thumbnails
//! - Disk cache: Persistent storage for thumbnails across sessions

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime};

use lru::LruCache;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{ImageFormat, ThumbnailData, ThumbnailSize};
use crate::core::error::{OsError, Result};

/// Default memory cache capacity (number of thumbnails)
const DEFAULT_MEMORY_CACHE_CAPACITY: usize = 500;

/// Default disk cache max size in bytes (100 MB)
const DEFAULT_DISK_CACHE_MAX_SIZE: u64 = 100 * 1024 * 1024;

/// Default thumbnail TTL (7 days)
const DEFAULT_TTL_SECS: u64 = 7 * 24 * 60 * 60;

/// Cache entry metadata stored on disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntryMetadata {
    /// Original file path
    pub file_path: String,
    /// File modification time when thumbnail was generated
    pub file_mtime: u64,
    /// Thumbnail size preset used
    pub size: ThumbnailSizeSerializable,
    /// Image format
    pub format: ImageFormatSerializable,
    /// Thumbnail width
    pub width: u32,
    /// Thumbnail height
    pub height: u32,
    /// When the cache entry was created
    pub created_at: u64,
    /// Size of the thumbnail data in bytes
    pub data_size: usize,
}

/// Serializable version of ThumbnailSize
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ThumbnailSizeSerializable {
    Small,
    Medium,
    Large,
    XLarge,
}

impl From<ThumbnailSize> for ThumbnailSizeSerializable {
    fn from(size: ThumbnailSize) -> Self {
        match size {
            ThumbnailSize::Small => Self::Small,
            ThumbnailSize::Medium => Self::Medium,
            ThumbnailSize::Large => Self::Large,
            ThumbnailSize::XLarge => Self::XLarge,
        }
    }
}

impl From<ThumbnailSizeSerializable> for ThumbnailSize {
    fn from(size: ThumbnailSizeSerializable) -> Self {
        match size {
            ThumbnailSizeSerializable::Small => Self::Small,
            ThumbnailSizeSerializable::Medium => Self::Medium,
            ThumbnailSizeSerializable::Large => Self::Large,
            ThumbnailSizeSerializable::XLarge => Self::XLarge,
        }
    }
}

/// Serializable version of ImageFormat
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ImageFormatSerializable {
    Png,
    Jpeg,
    Bmp,
}

impl From<ImageFormat> for ImageFormatSerializable {
    fn from(format: ImageFormat) -> Self {
        match format {
            ImageFormat::Png => Self::Png,
            ImageFormat::Jpeg => Self::Jpeg,
            ImageFormat::Bmp => Self::Bmp,
        }
    }
}

impl From<ImageFormatSerializable> for ImageFormat {
    fn from(format: ImageFormatSerializable) -> Self {
        match format {
            ImageFormatSerializable::Png => Self::Png,
            ImageFormatSerializable::Jpeg => Self::Jpeg,
            ImageFormatSerializable::Bmp => Self::Bmp,
        }
    }
}

/// Cache key combining file path and thumbnail size
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    /// Normalized file path
    pub path: String,
    /// Thumbnail size
    pub size: ThumbnailSizeSerializable,
}

impl CacheKey {
    /// Create a new cache key
    pub fn new(path: &Path, size: ThumbnailSize) -> Self {
        Self {
            path: path.to_string_lossy().to_string(),
            size: size.into(),
        }
    }

    /// Generate a unique filename for disk storage
    pub fn to_filename(&self) -> String {
        use blake3::Hasher;
        let mut hasher = Hasher::new();
        hasher.update(self.path.as_bytes());
        hasher.update(&[self.size as u8]);
        let hash = hasher.finalize();
        format!("{}.thumb", hash.to_hex())
    }
}

/// In-memory cache entry
#[derive(Debug, Clone)]
pub struct MemoryCacheEntry {
    /// Thumbnail data
    pub data: Arc<ThumbnailData>,
    /// When this entry was last accessed
    pub last_accessed: Instant,
    /// File modification time when thumbnail was generated
    pub file_mtime: u64,
}

/// Thumbnail cache configuration
#[derive(Debug, Clone)]
pub struct ThumbnailCacheConfig {
    /// Maximum number of thumbnails in memory cache
    pub memory_capacity: usize,
    /// Maximum disk cache size in bytes
    pub disk_max_size: u64,
    /// Time-to-live for cache entries in seconds
    pub ttl_secs: u64,
    /// Directory for disk cache storage
    pub cache_dir: PathBuf,
}

impl Default for ThumbnailCacheConfig {
    fn default() -> Self {
        Self {
            memory_capacity: DEFAULT_MEMORY_CACHE_CAPACITY,
            disk_max_size: DEFAULT_DISK_CACHE_MAX_SIZE,
            ttl_secs: DEFAULT_TTL_SECS,
            cache_dir: PathBuf::from("cache/thumbnails"),
        }
    }
}

impl ThumbnailCacheConfig {
    /// Create a new configuration with a custom cache directory
    pub fn with_cache_dir(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            ..Default::default()
        }
    }
}

/// Two-tier thumbnail cache with LRU memory cache and disk persistence
pub struct ThumbnailCache {
    /// Configuration
    config: ThumbnailCacheConfig,
    /// In-memory LRU cache
    memory_cache: RwLock<LruCache<CacheKey, MemoryCacheEntry>>,
    /// Current disk cache size in bytes
    disk_cache_size: RwLock<u64>,
}

impl ThumbnailCache {
    /// Create a new thumbnail cache with the given configuration
    pub fn new(config: ThumbnailCacheConfig) -> Result<Self> {
        // Ensure cache directory exists
        fs::create_dir_all(&config.cache_dir).map_err(|e| OsError::ThumbnailExtractionFailed {
            reason: format!("Failed to create cache directory: {}", e),
        })?;

        let memory_cache = RwLock::new(LruCache::new(
            std::num::NonZeroUsize::new(config.memory_capacity)
                .unwrap_or(std::num::NonZeroUsize::new(DEFAULT_MEMORY_CACHE_CAPACITY).unwrap()),
        ));

        let cache = Self {
            config,
            memory_cache,
            disk_cache_size: RwLock::new(0),
        };

        // Calculate initial disk cache size
        cache.recalculate_disk_size()?;

        Ok(cache)
    }

    /// Create a new thumbnail cache with default configuration
    pub fn with_default_config(cache_dir: PathBuf) -> Result<Self> {
        Self::new(ThumbnailCacheConfig::with_cache_dir(cache_dir))
    }

    /// Get a thumbnail from cache (memory first, then disk)
    pub fn get(&self, path: &Path, size: ThumbnailSize) -> Option<ThumbnailData> {
        let key = CacheKey::new(path, size);

        // Check memory cache first
        {
            let mut memory = self.memory_cache.write().ok()?;
            if let Some(entry) = memory.get(&key) {
                // Verify file hasn't changed
                if self.is_file_unchanged(path, entry.file_mtime) {
                    return Some((*entry.data).clone());
                } else {
                    // File changed, invalidate cache
                    memory.pop(&key);
                }
            }
        }

        // Check disk cache
        if let Some(data) = self.get_from_disk(&key, path) {
            // Promote to memory cache
            self.put_memory(&key, data.clone(), self.get_file_mtime(path).unwrap_or(0));
            return Some(data);
        }

        None
    }

    /// Put a thumbnail into cache (both memory and disk)
    pub fn put(&self, path: &Path, size: ThumbnailSize, data: ThumbnailData) -> Result<()> {
        let key = CacheKey::new(path, size);
        let file_mtime = self.get_file_mtime(path).unwrap_or(0);

        // Put in memory cache
        self.put_memory(&key, data.clone(), file_mtime);

        // Put in disk cache
        self.put_disk(&key, path, &data, file_mtime)?;

        Ok(())
    }

    /// Remove a thumbnail from cache
    pub fn remove(&self, path: &Path, size: ThumbnailSize) -> Result<()> {
        let key = CacheKey::new(path, size);

        // Remove from memory
        if let Ok(mut memory) = self.memory_cache.write() {
            memory.pop(&key);
        }

        // Remove from disk
        self.remove_from_disk(&key)?;

        Ok(())
    }

    /// Remove all cached thumbnails for a file (all sizes)
    pub fn remove_all_sizes(&self, path: &Path) -> Result<()> {
        for size in [
            ThumbnailSize::Small,
            ThumbnailSize::Medium,
            ThumbnailSize::Large,
            ThumbnailSize::XLarge,
        ] {
            let _ = self.remove(path, size);
        }
        Ok(())
    }

    /// Clear all cache entries
    pub fn clear(&self) -> Result<()> {
        // Clear memory cache
        if let Ok(mut memory) = self.memory_cache.write() {
            memory.clear();
        }

        // Clear disk cache
        self.clear_disk_cache()?;

        Ok(())
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let memory_count = self
            .memory_cache
            .read()
            .map(|m| m.len())
            .unwrap_or(0);
        let disk_size = *self.disk_cache_size.read().unwrap_or(&std::sync::RwLockReadGuard::map(
            self.disk_cache_size.read().unwrap(),
            |_| &0u64,
        ));

        CacheStats {
            memory_entries: memory_count,
            memory_capacity: self.config.memory_capacity,
            disk_size_bytes: disk_size,
            disk_max_bytes: self.config.disk_max_size,
        }
    }

    /// Evict expired entries from disk cache
    pub fn evict_expired(&self) -> Result<usize> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let ttl = self.config.ttl_secs;
        let mut evicted = 0;

        let entries = fs::read_dir(&self.config.cache_dir).map_err(|e| {
            OsError::ThumbnailExtractionFailed {
                reason: format!("Failed to read cache directory: {}", e),
            }
        })?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "meta").unwrap_or(false) {
                if let Ok(metadata) = self.read_metadata(&path) {
                    if now - metadata.created_at > ttl {
                        // Remove both metadata and data files
                        let data_path = path.with_extension("thumb");
                        let _ = fs::remove_file(&path);
                        let _ = fs::remove_file(&data_path);
                        evicted += 1;
                    }
                }
            }
        }

        self.recalculate_disk_size()?;
        Ok(evicted)
    }

    // Private helper methods

    fn put_memory(&self, key: &CacheKey, data: ThumbnailData, file_mtime: u64) {
        if let Ok(mut memory) = self.memory_cache.write() {
            memory.put(
                key.clone(),
                MemoryCacheEntry {
                    data: Arc::new(data),
                    last_accessed: Instant::now(),
                    file_mtime,
                },
            );
        }
    }

    fn get_from_disk(&self, key: &CacheKey, original_path: &Path) -> Option<ThumbnailData> {
        let filename = key.to_filename();
        let meta_path = self.config.cache_dir.join(format!("{}.meta", filename));
        let data_path = self.config.cache_dir.join(&filename);

        // Read metadata
        let metadata = self.read_metadata(&meta_path).ok()?;

        // Verify file hasn't changed
        if !self.is_file_unchanged(original_path, metadata.file_mtime) {
            // File changed, remove stale cache
            let _ = fs::remove_file(&meta_path);
            let _ = fs::remove_file(&data_path);
            return None;
        }

        // Check TTL
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if now - metadata.created_at > self.config.ttl_secs {
            // Expired, remove
            let _ = fs::remove_file(&meta_path);
            let _ = fs::remove_file(&data_path);
            return None;
        }

        // Read thumbnail data
        let mut file = File::open(&data_path).ok()?;
        let mut data = Vec::with_capacity(metadata.data_size);
        file.read_to_end(&mut data).ok()?;

        Some(ThumbnailData::new(
            data,
            metadata.format.into(),
            metadata.width,
            metadata.height,
        ))
    }

    fn put_disk(
        &self,
        key: &CacheKey,
        original_path: &Path,
        data: &ThumbnailData,
        file_mtime: u64,
    ) -> Result<()> {
        // Check if we need to evict entries
        self.ensure_disk_space(data.len() as u64)?;

        let filename = key.to_filename();
        let meta_path = self.config.cache_dir.join(format!("{}.meta", filename));
        let data_path = self.config.cache_dir.join(&filename);

        // Create metadata
        let metadata = CacheEntryMetadata {
            file_path: original_path.to_string_lossy().to_string(),
            file_mtime,
            size: key.size,
            format: data.format.into(),
            width: data.width,
            height: data.height,
            created_at: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            data_size: data.len(),
        };

        // Write metadata
        let meta_json = serde_json::to_string(&metadata).map_err(|e| {
            OsError::ThumbnailExtractionFailed {
                reason: format!("Failed to serialize metadata: {}", e),
            }
        })?;
        fs::write(&meta_path, meta_json).map_err(|e| OsError::ThumbnailExtractionFailed {
            reason: format!("Failed to write metadata: {}", e),
        })?;

        // Write thumbnail data
        fs::write(&data_path, &data.data).map_err(|e| OsError::ThumbnailExtractionFailed {
            reason: format!("Failed to write thumbnail data: {}", e),
        })?;

        // Update disk size
        if let Ok(mut size) = self.disk_cache_size.write() {
            *size += data.len() as u64 + meta_json.len() as u64;
        }

        Ok(())
    }

    fn remove_from_disk(&self, key: &CacheKey) -> Result<()> {
        let filename = key.to_filename();
        let meta_path = self.config.cache_dir.join(format!("{}.meta", filename));
        let data_path = self.config.cache_dir.join(&filename);

        let mut removed_size = 0u64;

        if let Ok(meta) = fs::metadata(&meta_path) {
            removed_size += meta.len();
            let _ = fs::remove_file(&meta_path);
        }

        if let Ok(meta) = fs::metadata(&data_path) {
            removed_size += meta.len();
            let _ = fs::remove_file(&data_path);
        }

        if let Ok(mut size) = self.disk_cache_size.write() {
            *size = size.saturating_sub(removed_size);
        }

        Ok(())
    }

    fn clear_disk_cache(&self) -> Result<()> {
        let entries = fs::read_dir(&self.config.cache_dir).map_err(|e| {
            OsError::ThumbnailExtractionFailed {
                reason: format!("Failed to read cache directory: {}", e),
            }
        })?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "thumb" || e == "meta").unwrap_or(false) {
                let _ = fs::remove_file(&path);
            }
        }

        if let Ok(mut size) = self.disk_cache_size.write() {
            *size = 0;
        }

        Ok(())
    }

    fn ensure_disk_space(&self, needed: u64) -> Result<()> {
        let current_size = *self.disk_cache_size.read().map_err(|_| {
            OsError::ThumbnailExtractionFailed {
                reason: "Failed to read disk cache size".to_string(),
            }
        })?;

        if current_size + needed <= self.config.disk_max_size {
            return Ok(());
        }

        // Need to evict entries - use LRU based on creation time
        let mut entries: Vec<(PathBuf, u64, u64)> = Vec::new(); // (path, created_at, size)

        let dir_entries = fs::read_dir(&self.config.cache_dir).map_err(|e| {
            OsError::ThumbnailExtractionFailed {
                reason: format!("Failed to read cache directory: {}", e),
            }
        })?;

        for entry in dir_entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "meta").unwrap_or(false) {
                if let Ok(metadata) = self.read_metadata(&path) {
                    let data_path = path.with_extension("thumb");
                    let total_size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
                        + fs::metadata(&data_path).map(|m| m.len()).unwrap_or(0);
                    entries.push((path, metadata.created_at, total_size));
                }
            }
        }

        // Sort by creation time (oldest first)
        entries.sort_by_key(|(_, created_at, _)| *created_at);

        // Evict until we have enough space
        let mut freed = 0u64;
        let target = needed + (self.config.disk_max_size / 10); // Free 10% extra

        for (meta_path, _, size) in entries {
            if current_size - freed + needed <= self.config.disk_max_size - target {
                break;
            }

            let data_path = meta_path.with_extension("thumb");
            let _ = fs::remove_file(&meta_path);
            let _ = fs::remove_file(&data_path);
            freed += size;
        }

        if let Ok(mut size) = self.disk_cache_size.write() {
            *size = size.saturating_sub(freed);
        }

        Ok(())
    }

    fn recalculate_disk_size(&self) -> Result<()> {
        let mut total_size = 0u64;

        let entries = fs::read_dir(&self.config.cache_dir).map_err(|e| {
            OsError::ThumbnailExtractionFailed {
                reason: format!("Failed to read cache directory: {}", e),
            }
        })?;

        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                total_size += meta.len();
            }
        }

        if let Ok(mut size) = self.disk_cache_size.write() {
            *size = total_size;
        }

        Ok(())
    }

    fn read_metadata(&self, path: &Path) -> Result<CacheEntryMetadata> {
        let content = fs::read_to_string(path).map_err(|e| OsError::ThumbnailExtractionFailed {
            reason: format!("Failed to read metadata: {}", e),
        })?;

        serde_json::from_str(&content).map_err(|e| {
            OsError::ThumbnailExtractionFailed {
                reason: format!("Failed to parse metadata: {}", e),
            }
            .into()
        })
    }

    fn get_file_mtime(&self, path: &Path) -> Option<u64> {
        fs::metadata(path)
            .ok()?
            .modified()
            .ok()?
            .duration_since(SystemTime::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs())
    }

    fn is_file_unchanged(&self, path: &Path, cached_mtime: u64) -> bool {
        self.get_file_mtime(path)
            .map(|current| current == cached_mtime)
            .unwrap_or(false)
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Number of entries in memory cache
    pub memory_entries: usize,
    /// Maximum memory cache capacity
    pub memory_capacity: usize,
    /// Current disk cache size in bytes
    pub disk_size_bytes: u64,
    /// Maximum disk cache size in bytes
    pub disk_max_bytes: u64,
}

impl CacheStats {
    /// Get memory cache utilization as a percentage
    pub fn memory_utilization(&self) -> f64 {
        if self.memory_capacity == 0 {
            0.0
        } else {
            (self.memory_entries as f64 / self.memory_capacity as f64) * 100.0
        }
    }

    /// Get disk cache utilization as a percentage
    pub fn disk_utilization(&self) -> f64 {
        if self.disk_max_bytes == 0 {
            0.0
        } else {
            (self.disk_size_bytes as f64 / self.disk_max_bytes as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_cache() -> (ThumbnailCache, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = ThumbnailCacheConfig {
            memory_capacity: 10,
            disk_max_size: 1024 * 1024, // 1 MB
            ttl_secs: 3600,
            cache_dir: temp_dir.path().to_path_buf(),
        };
        let cache = ThumbnailCache::new(config).unwrap();
        (cache, temp_dir)
    }

    fn create_test_thumbnail() -> ThumbnailData {
        ThumbnailData::new(vec![0u8; 100], ImageFormat::Png, 96, 96)
    }

    #[test]
    fn test_cache_key_creation() {
        let key = CacheKey::new(Path::new("/test/file.jpg"), ThumbnailSize::Medium);
        assert_eq!(key.path, "/test/file.jpg");
        assert_eq!(key.size, ThumbnailSizeSerializable::Medium);
    }

    #[test]
    fn test_cache_key_filename() {
        let key1 = CacheKey::new(Path::new("/test/file.jpg"), ThumbnailSize::Medium);
        let key2 = CacheKey::new(Path::new("/test/file.jpg"), ThumbnailSize::Large);
        let key3 = CacheKey::new(Path::new("/test/other.jpg"), ThumbnailSize::Medium);

        // Same path + size should produce same filename
        assert_eq!(key1.to_filename(), key1.to_filename());

        // Different size should produce different filename
        assert_ne!(key1.to_filename(), key2.to_filename());

        // Different path should produce different filename
        assert_ne!(key1.to_filename(), key3.to_filename());
    }

    #[test]
    fn test_cache_stats() {
        let (cache, _temp_dir) = create_test_cache();
        let stats = cache.stats();

        assert_eq!(stats.memory_entries, 0);
        assert_eq!(stats.memory_capacity, 10);
        assert_eq!(stats.disk_size_bytes, 0);
        assert_eq!(stats.memory_utilization(), 0.0);
        assert_eq!(stats.disk_utilization(), 0.0);
    }

    #[test]
    fn test_thumbnail_size_serializable_conversion() {
        let sizes = [
            ThumbnailSize::Small,
            ThumbnailSize::Medium,
            ThumbnailSize::Large,
            ThumbnailSize::XLarge,
        ];

        for size in sizes {
            let serializable: ThumbnailSizeSerializable = size.into();
            let back: ThumbnailSize = serializable.into();
            assert_eq!(size, back);
        }
    }

    #[test]
    fn test_image_format_serializable_conversion() {
        let formats = [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::Bmp];

        for format in formats {
            let serializable: ImageFormatSerializable = format.into();
            let back: ImageFormat = serializable.into();
            assert_eq!(format, back);
        }
    }

    #[test]
    fn test_cache_config_default() {
        let config = ThumbnailCacheConfig::default();
        assert_eq!(config.memory_capacity, DEFAULT_MEMORY_CACHE_CAPACITY);
        assert_eq!(config.disk_max_size, DEFAULT_DISK_CACHE_MAX_SIZE);
        assert_eq!(config.ttl_secs, DEFAULT_TTL_SECS);
    }
}
