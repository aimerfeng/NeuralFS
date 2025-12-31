//! Model download management for NeuralFS
//!
//! Provides multi-source model downloading with:
//! - Resume capability (Range requests)
//! - SHA256 checksum verification
//! - Progress reporting
//! - Automatic source failover

use chrono::{DateTime, Utc};
use futures::StreamExt;
use reqwest::header::{HeaderValue, CONTENT_LENGTH, RANGE};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::RwLock;

/// Error types for model downloading
#[derive(Error, Debug)]
pub enum DownloadError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("All download sources failed for model: {model_id}")]
    AllSourcesFailed { model_id: String },

    #[error("Checksum mismatch for {filename}: expected {expected}, got {actual}")]
    ChecksumMismatch {
        filename: String,
        expected: String,
        actual: String,
    },

    #[error("Manifest fetch failed: {reason}")]
    ManifestFetchFailed { reason: String },

    #[error("Model not found in manifest: {model_id}")]
    ModelNotFound { model_id: String },

    #[error("Download cancelled")]
    Cancelled,

    #[error("Invalid response: {reason}")]
    InvalidResponse { reason: String },

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Result type for download operations
pub type Result<T> = std::result::Result<T, DownloadError>;


/// Model type enumeration
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ModelType {
    /// Text embedding model (e.g., all-MiniLM-L6-v2)
    TextEmbedding,
    /// Image embedding model (e.g., CLIP)
    ImageEmbedding,
    /// Intent parsing model
    IntentParser,
    /// Tokenizer vocabulary
    Tokenizer,
}

impl std::fmt::Display for ModelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelType::TextEmbedding => write!(f, "text_embedding"),
            ModelType::ImageEmbedding => write!(f, "image_embedding"),
            ModelType::IntentParser => write!(f, "intent_parser"),
            ModelType::Tokenizer => write!(f, "tokenizer"),
        }
    }
}

/// Download source configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSource {
    /// Source name for display
    pub name: String,
    /// Base URL for downloads
    pub base_url: String,
    /// Priority (lower = higher priority)
    pub priority: u32,
    /// Whether this source is currently available
    pub available: bool,
}

impl ModelSource {
    /// Create a new model source
    pub fn new(name: impl Into<String>, base_url: impl Into<String>, priority: u32) -> Self {
        Self {
            name: name.into(),
            base_url: base_url.into(),
            priority,
            available: true,
        }
    }
}

/// Information about a downloadable model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Unique model identifier
    pub id: String,
    /// Display name
    pub name: String,
    /// Model type
    pub model_type: ModelType,
    /// Filename for storage
    pub filename: String,
    /// File size in bytes
    pub size_bytes: u64,
    /// SHA256 checksum for verification
    pub sha256: String,
    /// Whether this model is required for basic functionality
    pub required: bool,
    /// Human-readable description
    pub description: String,
    /// VRAM requirement in MB
    pub vram_mb: u32,
}

/// Model manifest containing available models
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelManifest {
    /// List of available models
    pub models: Vec<ModelInfo>,
    /// Manifest version
    pub version: String,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
}

impl Default for ModelManifest {
    fn default() -> Self {
        Self {
            models: vec![
                ModelInfo {
                    id: "all-minilm-l6-v2".to_string(),
                    name: "All-MiniLM-L6-v2".to_string(),
                    model_type: ModelType::TextEmbedding,
                    filename: "all-minilm-l6-v2.onnx".to_string(),
                    size_bytes: 90_000_000, // ~90MB
                    sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
                    required: true,
                    description: "Lightweight text embedding model for semantic search".to_string(),
                    vram_mb: 256,
                },
                ModelInfo {
                    id: "clip-vit-base".to_string(),
                    name: "CLIP ViT-Base".to_string(),
                    model_type: ModelType::ImageEmbedding,
                    filename: "clip-vit-base.onnx".to_string(),
                    size_bytes: 350_000_000, // ~350MB
                    sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
                    required: false,
                    description: "Image embedding model for visual search".to_string(),
                    vram_mb: 512,
                },
            ],
            version: "1.0.0".to_string(),
            updated_at: Utc::now(),
        }
    }
}


/// Download progress information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    /// Model being downloaded
    pub model_id: String,
    /// Bytes downloaded so far
    pub downloaded: u64,
    /// Total bytes to download
    pub total: u64,
    /// Download percentage (0-100)
    pub percentage: u8,
    /// Current download speed in bytes/second
    pub speed_bps: u64,
    /// Estimated time remaining in seconds
    pub eta_seconds: Option<u64>,
    /// Current source being used
    pub source_name: String,
}

impl DownloadProgress {
    /// Calculate percentage from downloaded and total
    pub fn calculate_percentage(downloaded: u64, total: u64) -> u8 {
        if total == 0 {
            return 0;
        }
        ((downloaded as f64 / total as f64) * 100.0).min(100.0) as u8
    }
}

/// Download status for tracking
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DownloadStatus {
    /// Not started
    Pending,
    /// Currently downloading
    Downloading,
    /// Download paused (can resume)
    Paused,
    /// Verifying checksum
    Verifying,
    /// Successfully completed
    Completed,
    /// Failed with error
    Failed { reason: String },
    /// Cancelled by user
    Cancelled,
}

/// Configuration for the model downloader
#[derive(Debug, Clone)]
pub struct ModelDownloaderConfig {
    /// Directory to store downloaded models
    pub models_dir: PathBuf,
    /// Download sources in priority order
    pub sources: Vec<ModelSource>,
    /// HTTP request timeout
    pub timeout: Duration,
    /// Maximum retry attempts per source
    pub max_retries: u32,
    /// Chunk size for streaming downloads (bytes)
    pub chunk_size: usize,
    /// Manifest URL (optional, for remote manifest)
    pub manifest_url: Option<String>,
}

impl Default for ModelDownloaderConfig {
    fn default() -> Self {
        let models_dir = directories::BaseDirs::new()
            .map(|dirs| dirs.data_local_dir().join("NeuralFS").join("models"))
            .unwrap_or_else(|| PathBuf::from("models"));

        Self {
            models_dir,
            sources: vec![
                // China mirror (highest priority for users in China)
                ModelSource::new(
                    "HuggingFace Mirror (China)",
                    "https://hf-mirror.com",
                    1,
                ),
                // Self-hosted CDN
                ModelSource::new(
                    "NeuralFS CDN",
                    "https://models.neuralfs.io",
                    2,
                ),
                // Official HuggingFace
                ModelSource::new(
                    "HuggingFace",
                    "https://huggingface.co",
                    3,
                ),
            ],
            timeout: Duration::from_secs(300),
            max_retries: 3,
            chunk_size: 1024 * 1024, // 1MB chunks
            manifest_url: None,
        }
    }
}


/// Progress callback type
pub type ProgressCallback = Arc<dyn Fn(DownloadProgress) + Send + Sync>;

/// Model downloader with multi-source support and resume capability
pub struct ModelDownloader {
    /// Configuration
    config: ModelDownloaderConfig,
    /// HTTP client
    client: reqwest::Client,
    /// Progress callback
    progress_callback: Option<ProgressCallback>,
    /// Current download status per model
    status: Arc<RwLock<std::collections::HashMap<String, DownloadStatus>>>,
    /// Cancellation flag
    cancelled: Arc<RwLock<bool>>,
}

impl ModelDownloader {
    /// Create a new model downloader with default configuration
    pub fn new() -> Result<Self> {
        Self::with_config(ModelDownloaderConfig::default())
    }

    /// Create a new model downloader with custom configuration
    pub fn with_config(config: ModelDownloaderConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()?;

        Ok(Self {
            config,
            client,
            progress_callback: None,
            status: Arc::new(RwLock::new(std::collections::HashMap::new())),
            cancelled: Arc::new(RwLock::new(false)),
        })
    }

    /// Set progress callback
    pub fn with_progress_callback(mut self, callback: ProgressCallback) -> Self {
        self.progress_callback = Some(callback);
        self
    }

    /// Get the models directory
    pub fn models_dir(&self) -> &Path {
        &self.config.models_dir
    }

    /// Get configured sources
    pub fn sources(&self) -> &[ModelSource] {
        &self.config.sources
    }

    /// Cancel ongoing downloads
    pub async fn cancel(&self) {
        let mut cancelled = self.cancelled.write().await;
        *cancelled = true;
    }

    /// Reset cancellation flag
    pub async fn reset_cancel(&self) {
        let mut cancelled = self.cancelled.write().await;
        *cancelled = false;
    }

    /// Check if download is cancelled
    async fn is_cancelled(&self) -> bool {
        *self.cancelled.read().await
    }

    /// Get download status for a model
    pub async fn get_status(&self, model_id: &str) -> Option<DownloadStatus> {
        self.status.read().await.get(model_id).cloned()
    }

    /// Set download status for a model
    async fn set_status(&self, model_id: &str, status: DownloadStatus) {
        self.status.write().await.insert(model_id.to_string(), status);
    }

    /// Fetch model manifest from remote or return default
    pub async fn fetch_manifest(&self) -> Result<ModelManifest> {
        if let Some(url) = &self.config.manifest_url {
            let response = self.client.get(url).send().await?;
            if response.status().is_success() {
                let manifest: ModelManifest = response.json().await?;
                return Ok(manifest);
            }
        }
        // Return default manifest if no URL or fetch failed
        Ok(ModelManifest::default())
    }

    /// Check if a model is present and valid
    pub fn is_model_present(&self, model: &ModelInfo) -> bool {
        let path = self.config.models_dir.join(&model.filename);
        if !path.exists() {
            return false;
        }
        // Check file size as quick validation
        if let Ok(metadata) = std::fs::metadata(&path) {
            return metadata.len() == model.size_bytes;
        }
        false
    }

    /// Get path for a model file
    pub fn get_model_path(&self, model: &ModelInfo) -> PathBuf {
        self.config.models_dir.join(&model.filename)
    }

    /// Get path for partial download file
    fn get_partial_path(&self, model: &ModelInfo) -> PathBuf {
        self.config.models_dir.join(format!("{}.part", model.filename))
    }

    /// Ensure all required models are downloaded
    pub async fn ensure_models(&self) -> Result<Vec<PathBuf>> {
        let manifest = self.fetch_manifest().await?;
        let mut downloaded = Vec::new();

        for model in &manifest.models {
            if model.required && !self.is_model_present(model) {
                let path = self.download_model(model).await?;
                downloaded.push(path);
            }
        }

        Ok(downloaded)
    }

    /// Download a specific model by ID
    pub async fn download_model_by_id(&self, model_id: &str) -> Result<PathBuf> {
        let manifest = self.fetch_manifest().await?;
        let model = manifest
            .models
            .iter()
            .find(|m| m.id == model_id)
            .ok_or_else(|| DownloadError::ModelNotFound {
                model_id: model_id.to_string(),
            })?;
        self.download_model(model).await
    }


    /// Download a model with automatic source failover
    pub async fn download_model(&self, model: &ModelInfo) -> Result<PathBuf> {
        // Ensure models directory exists
        tokio::fs::create_dir_all(&self.config.models_dir).await?;

        let target_path = self.get_model_path(model);
        let partial_path = self.get_partial_path(model);

        self.set_status(&model.id, DownloadStatus::Downloading).await;

        // Sort sources by priority
        let mut sources: Vec<_> = self.config.sources.iter()
            .filter(|s| s.available)
            .collect();
        sources.sort_by_key(|s| s.priority);

        let mut last_error = None;

        for source in sources {
            if self.is_cancelled().await {
                self.set_status(&model.id, DownloadStatus::Cancelled).await;
                return Err(DownloadError::Cancelled);
            }

            let url = format!("{}/{}", source.base_url, model.filename);
            tracing::info!("Attempting download from {}: {}", source.name, url);

            for attempt in 0..self.config.max_retries {
                if self.is_cancelled().await {
                    self.set_status(&model.id, DownloadStatus::Cancelled).await;
                    return Err(DownloadError::Cancelled);
                }

                match self.download_file(&url, &partial_path, model, &source.name).await {
                    Ok(_) => {
                        // Verify checksum
                        self.set_status(&model.id, DownloadStatus::Verifying).await;
                        match self.verify_checksum(&partial_path, &model.sha256).await {
                            Ok(true) => {
                                // Rename to final path
                                tokio::fs::rename(&partial_path, &target_path).await?;
                                self.set_status(&model.id, DownloadStatus::Completed).await;
                                tracing::info!("Successfully downloaded model: {}", model.id);
                                return Ok(target_path);
                            }
                            Ok(false) => {
                                // Checksum mismatch - delete and try again
                                tracing::warn!("Checksum mismatch for {}, retrying...", model.filename);
                                tokio::fs::remove_file(&partial_path).await.ok();
                                last_error = Some(DownloadError::ChecksumMismatch {
                                    filename: model.filename.clone(),
                                    expected: model.sha256.clone(),
                                    actual: "mismatch".to_string(),
                                });
                            }
                            Err(e) => {
                                tracing::warn!("Checksum verification failed: {}", e);
                                last_error = Some(e);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Download attempt {}/{} from {} failed: {}",
                            attempt + 1,
                            self.config.max_retries,
                            source.name,
                            e
                        );
                        last_error = Some(e);
                        // Exponential backoff
                        if attempt < self.config.max_retries - 1 {
                            tokio::time::sleep(Duration::from_millis(500 * (1 << attempt))).await;
                        }
                    }
                }
            }
        }

        let error = last_error.unwrap_or(DownloadError::AllSourcesFailed {
            model_id: model.id.clone(),
        });
        self.set_status(&model.id, DownloadStatus::Failed {
            reason: error.to_string(),
        }).await;
        Err(error)
    }

    /// Download a file with resume support
    async fn download_file(
        &self,
        url: &str,
        target: &Path,
        model: &ModelInfo,
        source_name: &str,
    ) -> Result<()> {
        let mut downloaded: u64 = 0;

        // Check for existing partial download
        if target.exists() {
            let metadata = tokio::fs::metadata(target).await?;
            downloaded = metadata.len();
            tracing::info!("Resuming download from byte {}", downloaded);
        }

        // Build request with Range header for resume
        let mut request = self.client.get(url);
        if downloaded > 0 {
            request = request.header(RANGE, format!("bytes={}-", downloaded));
        }

        let response = request.send().await?;

        // Check response status
        if !response.status().is_success() && response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            return Err(DownloadError::InvalidResponse {
                reason: format!("HTTP {}", response.status()),
            });
        }

        // Get total size from Content-Length or Content-Range
        let total_size = if downloaded > 0 {
            model.size_bytes
        } else {
            response
                .headers()
                .get(CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok())
                .unwrap_or(model.size_bytes)
        };

        // Open file for appending
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(target)
            .await?;

        // Stream download
        let mut stream = response.bytes_stream();
        let start_time = std::time::Instant::now();
        let mut last_progress_time = start_time;

        while let Some(chunk_result) = stream.next().await {
            if self.is_cancelled().await {
                return Err(DownloadError::Cancelled);
            }

            let chunk = chunk_result?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;

            // Report progress (throttled to avoid too many updates)
            let now = std::time::Instant::now();
            if now.duration_since(last_progress_time) >= Duration::from_millis(100) {
                last_progress_time = now;
                
                let elapsed = now.duration_since(start_time).as_secs_f64();
                let speed_bps = if elapsed > 0.0 {
                    (downloaded as f64 / elapsed) as u64
                } else {
                    0
                };
                
                let eta_seconds = if speed_bps > 0 && downloaded < total_size {
                    Some((total_size - downloaded) / speed_bps)
                } else {
                    None
                };

                let progress = DownloadProgress {
                    model_id: model.id.clone(),
                    downloaded,
                    total: total_size,
                    percentage: DownloadProgress::calculate_percentage(downloaded, total_size),
                    speed_bps,
                    eta_seconds,
                    source_name: source_name.to_string(),
                };

                if let Some(callback) = &self.progress_callback {
                    callback(progress);
                }
            }
        }

        // Ensure all data is flushed
        file.flush().await?;

        Ok(())
    }


    /// Verify file checksum
    pub async fn verify_checksum(&self, path: &Path, expected: &str) -> Result<bool> {
        let mut file = File::open(path).await?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; self.config.chunk_size];

        loop {
            let n = file.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }

        let result = format!("{:x}", hasher.finalize());
        Ok(result == expected)
    }

    /// Calculate SHA256 checksum of a file
    pub async fn calculate_checksum(&self, path: &Path) -> Result<String> {
        let mut file = File::open(path).await?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; self.config.chunk_size];

        loop {
            let n = file.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }

        Ok(format!("{:x}", hasher.finalize()))
    }

    /// Delete partial download file
    pub async fn cleanup_partial(&self, model: &ModelInfo) -> Result<()> {
        let partial_path = self.get_partial_path(model);
        if partial_path.exists() {
            tokio::fs::remove_file(&partial_path).await?;
        }
        Ok(())
    }

    /// Get list of all partial downloads
    pub async fn list_partial_downloads(&self) -> Result<Vec<PathBuf>> {
        let mut partials = Vec::new();
        
        if !self.config.models_dir.exists() {
            return Ok(partials);
        }

        let mut entries = tokio::fs::read_dir(&self.config.models_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().map(|e| e == "part").unwrap_or(false) {
                partials.push(path);
            }
        }

        Ok(partials)
    }

    /// Get download progress for resumable download
    pub async fn get_resume_info(&self, model: &ModelInfo) -> Option<(u64, u64)> {
        let partial_path = self.get_partial_path(model);
        if partial_path.exists() {
            if let Ok(metadata) = tokio::fs::metadata(&partial_path).await {
                return Some((metadata.len(), model.size_bytes));
            }
        }
        None
    }
}

impl Default for ModelDownloader {
    fn default() -> Self {
        Self::new().expect("Failed to create default ModelDownloader")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_model_type_display() {
        assert_eq!(ModelType::TextEmbedding.to_string(), "text_embedding");
        assert_eq!(ModelType::ImageEmbedding.to_string(), "image_embedding");
        assert_eq!(ModelType::IntentParser.to_string(), "intent_parser");
        assert_eq!(ModelType::Tokenizer.to_string(), "tokenizer");
    }

    #[test]
    fn test_model_source_creation() {
        let source = ModelSource::new("Test Source", "https://example.com", 1);
        assert_eq!(source.name, "Test Source");
        assert_eq!(source.base_url, "https://example.com");
        assert_eq!(source.priority, 1);
        assert!(source.available);
    }

    #[test]
    fn test_download_progress_percentage() {
        assert_eq!(DownloadProgress::calculate_percentage(0, 100), 0);
        assert_eq!(DownloadProgress::calculate_percentage(50, 100), 50);
        assert_eq!(DownloadProgress::calculate_percentage(100, 100), 100);
        assert_eq!(DownloadProgress::calculate_percentage(150, 100), 100); // Capped at 100
        assert_eq!(DownloadProgress::calculate_percentage(0, 0), 0); // Edge case
    }

    #[test]
    fn test_default_config() {
        let config = ModelDownloaderConfig::default();
        assert!(!config.sources.is_empty());
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.chunk_size, 1024 * 1024);
    }

    #[test]
    fn test_default_manifest() {
        let manifest = ModelManifest::default();
        assert!(!manifest.models.is_empty());
        assert!(manifest.models.iter().any(|m| m.required));
    }

    #[tokio::test]
    async fn test_downloader_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = ModelDownloaderConfig {
            models_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        
        let downloader = ModelDownloader::with_config(config).unwrap();
        assert_eq!(downloader.models_dir(), temp_dir.path());
        assert!(!downloader.sources().is_empty());
    }

    #[tokio::test]
    async fn test_model_not_present() {
        let temp_dir = TempDir::new().unwrap();
        let config = ModelDownloaderConfig {
            models_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        
        let downloader = ModelDownloader::with_config(config).unwrap();
        let model = ModelInfo {
            id: "test-model".to_string(),
            name: "Test Model".to_string(),
            model_type: ModelType::TextEmbedding,
            filename: "test.onnx".to_string(),
            size_bytes: 1000,
            sha256: "abc123".to_string(),
            required: true,
            description: "Test".to_string(),
            vram_mb: 100,
        };
        
        assert!(!downloader.is_model_present(&model));
    }

    #[tokio::test]
    async fn test_checksum_calculation() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        tokio::fs::write(&test_file, b"hello world").await.unwrap();
        
        let config = ModelDownloaderConfig {
            models_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        
        let downloader = ModelDownloader::with_config(config).unwrap();
        let checksum = downloader.calculate_checksum(&test_file).await.unwrap();
        
        // SHA256 of "hello world"
        assert_eq!(
            checksum,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[tokio::test]
    async fn test_checksum_verification() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        tokio::fs::write(&test_file, b"hello world").await.unwrap();
        
        let config = ModelDownloaderConfig {
            models_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        
        let downloader = ModelDownloader::with_config(config).unwrap();
        
        // Correct checksum
        let valid = downloader
            .verify_checksum(
                &test_file,
                "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9",
            )
            .await
            .unwrap();
        assert!(valid);
        
        // Wrong checksum
        let invalid = downloader
            .verify_checksum(&test_file, "wrong_checksum")
            .await
            .unwrap();
        assert!(!invalid);
    }

    #[tokio::test]
    async fn test_cancel_flag() {
        let temp_dir = TempDir::new().unwrap();
        let config = ModelDownloaderConfig {
            models_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        
        let downloader = ModelDownloader::with_config(config).unwrap();
        
        assert!(!downloader.is_cancelled().await);
        downloader.cancel().await;
        assert!(downloader.is_cancelled().await);
        downloader.reset_cancel().await;
        assert!(!downloader.is_cancelled().await);
    }

    #[tokio::test]
    async fn test_status_tracking() {
        let temp_dir = TempDir::new().unwrap();
        let config = ModelDownloaderConfig {
            models_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        
        let downloader = ModelDownloader::with_config(config).unwrap();
        
        assert!(downloader.get_status("test-model").await.is_none());
        
        downloader.set_status("test-model", DownloadStatus::Downloading).await;
        assert_eq!(
            downloader.get_status("test-model").await,
            Some(DownloadStatus::Downloading)
        );
        
        downloader.set_status("test-model", DownloadStatus::Completed).await;
        assert_eq!(
            downloader.get_status("test-model").await,
            Some(DownloadStatus::Completed)
        );
    }

    #[tokio::test]
    async fn test_partial_download_listing() {
        let temp_dir = TempDir::new().unwrap();
        
        // Create some partial files
        tokio::fs::write(temp_dir.path().join("model1.onnx.part"), b"partial1").await.unwrap();
        tokio::fs::write(temp_dir.path().join("model2.onnx.part"), b"partial2").await.unwrap();
        tokio::fs::write(temp_dir.path().join("complete.onnx"), b"complete").await.unwrap();
        
        let config = ModelDownloaderConfig {
            models_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        
        let downloader = ModelDownloader::with_config(config).unwrap();
        let partials = downloader.list_partial_downloads().await.unwrap();
        
        assert_eq!(partials.len(), 2);
        assert!(partials.iter().all(|p| p.extension().unwrap() == "part"));
    }
}
