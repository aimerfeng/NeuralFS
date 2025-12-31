//! Configuration Storage Implementation
//!
//! Provides JSON file-based configuration storage with:
//! - Atomic writes using temp file + rename
//! - Automatic backup before writes
//! - Thread-safe access via RwLock
//! - Default configuration generation
//!
//! **Validates: Requirements 15.7**

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use thiserror::Error;

/// Configuration error types
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Configuration file not found: {0}")]
    NotFound(PathBuf),

    #[error("Invalid configuration: {0}")]
    Invalid(String),

    #[error("Migration error: {0}")]
    Migration(String),

    #[error("Backup error: {0}")]
    Backup(String),
}

/// Configuration result type
pub type ConfigResult<T> = Result<T, ConfigError>;

/// Configuration store settings
#[derive(Debug, Clone)]
pub struct ConfigStoreConfig {
    /// Path to the configuration file
    pub config_path: PathBuf,
    /// Path to backup directory
    pub backup_dir: PathBuf,
    /// Maximum number of backups to keep
    pub max_backups: usize,
    /// Whether to create default config if not exists
    pub create_default: bool,
}

impl Default for ConfigStoreConfig {
    fn default() -> Self {
        let app_data = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("neuralfs");

        Self {
            config_path: app_data.join("config.json"),
            backup_dir: app_data.join("backups"),
            max_backups: 5,
            create_default: true,
        }
    }
}

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Configuration version for migration
    #[serde(default = "default_version")]
    pub version: u32,

    /// Directories to monitor for file changes
    #[serde(default)]
    pub monitored_directories: Vec<PathBuf>,

    /// Cloud API configuration
    #[serde(default)]
    pub cloud: CloudConfig,

    /// Performance settings
    #[serde(default)]
    pub performance: PerformanceConfig,

    /// Privacy settings
    #[serde(default)]
    pub privacy: PrivacyConfig,

    /// UI preferences
    #[serde(default)]
    pub ui: UIConfig,

    /// Last modified timestamp
    #[serde(default = "default_timestamp")]
    pub last_modified: String,
}

fn default_version() -> u32 {
    1
}

fn default_timestamp() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Cloud API configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudConfig {
    /// Whether cloud features are enabled
    #[serde(default)]
    pub enabled: bool,

    /// API endpoint URL
    #[serde(default)]
    pub endpoint: Option<String>,

    /// API key (encrypted in storage)
    #[serde(default)]
    pub api_key: Option<String>,

    /// Monthly cost limit in USD
    #[serde(default = "default_monthly_limit")]
    pub monthly_cost_limit: f64,

    /// Requests per minute limit
    #[serde(default = "default_rpm")]
    pub requests_per_minute: u32,

    /// Cloud model to use
    #[serde(default = "default_model")]
    pub model: String,

    /// Provider (openai, anthropic, custom)
    #[serde(default = "default_provider")]
    pub provider: String,
}

fn default_monthly_limit() -> f64 {
    10.0
}

fn default_rpm() -> u32 {
    60
}

fn default_model() -> String {
    "gpt-4o-mini".to_string()
}

fn default_provider() -> String {
    "openai".to_string()
}

impl Default for CloudConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: None,
            api_key: None,
            monthly_cost_limit: default_monthly_limit(),
            requests_per_minute: default_rpm(),
            model: default_model(),
            provider: default_provider(),
        }
    }
}

/// Performance tuning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Maximum VRAM usage in MB
    #[serde(default = "default_vram")]
    pub max_vram_mb: u32,

    /// Number of indexing threads
    #[serde(default = "default_threads")]
    pub indexing_threads: u32,

    /// Batch size for embedding generation
    #[serde(default = "default_batch")]
    pub embedding_batch_size: u32,

    /// Enable CUDA acceleration
    #[serde(default = "default_cuda")]
    pub enable_cuda: bool,

    /// Enable fast inference mode
    #[serde(default = "default_fast_mode")]
    pub fast_inference_mode: bool,
}

fn default_vram() -> u32 {
    4096
}

fn default_threads() -> u32 {
    4
}

fn default_batch() -> u32 {
    32
}

fn default_cuda() -> bool {
    true
}

fn default_fast_mode() -> bool {
    true
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            max_vram_mb: default_vram(),
            indexing_threads: default_threads(),
            embedding_batch_size: default_batch(),
            enable_cuda: default_cuda(),
            fast_inference_mode: default_fast_mode(),
        }
    }
}

/// Privacy settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyConfig {
    /// Enable privacy mode (disables all cloud features)
    #[serde(default)]
    pub privacy_mode: bool,

    /// Directories to exclude from indexing
    #[serde(default)]
    pub excluded_directories: Vec<PathBuf>,

    /// File patterns to exclude
    #[serde(default = "default_patterns")]
    pub excluded_patterns: Vec<String>,

    /// Enable telemetry (anonymous usage stats)
    #[serde(default)]
    pub enable_telemetry: bool,
}

fn default_patterns() -> Vec<String> {
    vec![
        "*.tmp".to_string(),
        "*.log".to_string(),
        "node_modules".to_string(),
        ".git".to_string(),
        "__pycache__".to_string(),
        "*.pyc".to_string(),
        ".DS_Store".to_string(),
        "Thumbs.db".to_string(),
    ]
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            privacy_mode: false,
            excluded_directories: vec![],
            excluded_patterns: default_patterns(),
            enable_telemetry: false,
        }
    }
}

/// UI preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UIConfig {
    /// Theme (light/dark/system)
    #[serde(default = "default_theme")]
    pub theme: String,

    /// Language code
    #[serde(default = "default_language")]
    pub language: String,

    /// Enable animations
    #[serde(default = "default_animations")]
    pub enable_animations: bool,

    /// Show file extensions
    #[serde(default = "default_show_ext")]
    pub show_extensions: bool,

    /// Default view mode (grid/list)
    #[serde(default = "default_view")]
    pub default_view: String,

    /// Thumbnail size (small/medium/large)
    #[serde(default = "default_thumb_size")]
    pub thumbnail_size: String,
}

fn default_theme() -> String {
    "dark".to_string()
}

fn default_language() -> String {
    "zh-CN".to_string()
}

fn default_animations() -> bool {
    true
}

fn default_show_ext() -> bool {
    true
}

fn default_view() -> String {
    "grid".to_string()
}

fn default_thumb_size() -> String {
    "medium".to_string()
}

impl Default for UIConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            language: default_language(),
            enable_animations: default_animations(),
            show_extensions: default_show_ext(),
            default_view: default_view(),
            thumbnail_size: default_thumb_size(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: default_version(),
            monitored_directories: vec![],
            cloud: CloudConfig::default(),
            performance: PerformanceConfig::default(),
            privacy: PrivacyConfig::default(),
            ui: UIConfig::default(),
            last_modified: default_timestamp(),
        }
    }
}


/// Configuration store with thread-safe access
pub struct ConfigStore {
    config: Arc<RwLock<AppConfig>>,
    settings: ConfigStoreConfig,
}

impl ConfigStore {
    /// Create a new configuration store
    pub async fn new(settings: ConfigStoreConfig) -> ConfigResult<Self> {
        // Ensure directories exist
        if let Some(parent) = settings.config_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::create_dir_all(&settings.backup_dir).await?;

        // Load or create config
        let config = if settings.config_path.exists() {
            Self::load_from_file(&settings.config_path).await?
        } else if settings.create_default {
            let default_config = AppConfig::default();
            Self::save_to_file(&settings.config_path, &default_config).await?;
            default_config
        } else {
            return Err(ConfigError::NotFound(settings.config_path.clone()));
        };

        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            settings,
        })
    }

    /// Load configuration from file
    async fn load_from_file(path: &Path) -> ConfigResult<AppConfig> {
        let content = tokio::fs::read_to_string(path).await?;
        let config: AppConfig = serde_json::from_str(&content)?;
        Ok(config)
    }

    /// Save configuration to file with atomic write
    async fn save_to_file(path: &Path, config: &AppConfig) -> ConfigResult<()> {
        let content = serde_json::to_string_pretty(config)?;
        
        // Write to temp file first
        let temp_path = path.with_extension("json.tmp");
        tokio::fs::write(&temp_path, &content).await?;
        
        // Atomic rename
        tokio::fs::rename(&temp_path, path).await?;
        
        Ok(())
    }

    /// Get current configuration (read-only)
    pub async fn get(&self) -> AppConfig {
        self.config.read().await.clone()
    }

    /// Update configuration
    pub async fn update<F>(&self, updater: F) -> ConfigResult<AppConfig>
    where
        F: FnOnce(&mut AppConfig),
    {
        let mut config = self.config.write().await;
        
        // Create backup before modifying
        self.create_backup(&config).await?;
        
        // Apply update
        updater(&mut config);
        config.last_modified = chrono::Utc::now().to_rfc3339();
        
        // Save to file
        Self::save_to_file(&self.settings.config_path, &config).await?;
        
        Ok(config.clone())
    }

    /// Set entire configuration
    pub async fn set(&self, new_config: AppConfig) -> ConfigResult<()> {
        let mut config = self.config.write().await;
        
        // Create backup before modifying
        self.create_backup(&config).await?;
        
        // Update config
        *config = new_config;
        config.last_modified = chrono::Utc::now().to_rfc3339();
        
        // Save to file
        Self::save_to_file(&self.settings.config_path, &config).await?;
        
        Ok(())
    }

    /// Create a backup of current configuration
    async fn create_backup(&self, config: &AppConfig) -> ConfigResult<()> {
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let backup_name = format!("config_backup_{}.json", timestamp);
        let backup_path = self.settings.backup_dir.join(backup_name);
        
        Self::save_to_file(&backup_path, config).await?;
        
        // Clean up old backups
        self.cleanup_old_backups().await?;
        
        Ok(())
    }

    /// Remove old backups exceeding max_backups limit
    async fn cleanup_old_backups(&self) -> ConfigResult<()> {
        let mut entries = tokio::fs::read_dir(&self.settings.backup_dir).await?;
        let mut backups: Vec<PathBuf> = Vec::new();
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                backups.push(path);
            }
        }
        
        // Sort by name (which includes timestamp)
        backups.sort();
        
        // Remove oldest backups if exceeding limit
        while backups.len() > self.settings.max_backups {
            if let Some(oldest) = backups.first() {
                tokio::fs::remove_file(oldest).await?;
                backups.remove(0);
            }
        }
        
        Ok(())
    }

    /// Export configuration to a file
    pub async fn export(&self, path: &Path) -> ConfigResult<()> {
        let config = self.config.read().await;
        Self::save_to_file(path, &config).await
    }

    /// Import configuration from a file
    pub async fn import(&self, path: &Path) -> ConfigResult<AppConfig> {
        let imported = Self::load_from_file(path).await?;
        self.set(imported.clone()).await?;
        Ok(imported)
    }

    /// Reset to default configuration
    pub async fn reset(&self) -> ConfigResult<AppConfig> {
        let default_config = AppConfig::default();
        self.set(default_config.clone()).await?;
        Ok(default_config)
    }

    /// Get configuration file path
    pub fn config_path(&self) -> &Path {
        &self.settings.config_path
    }

    /// Get backup directory path
    pub fn backup_dir(&self) -> &Path {
        &self.settings.backup_dir
    }

    /// List available backups
    pub async fn list_backups(&self) -> ConfigResult<Vec<PathBuf>> {
        let mut entries = tokio::fs::read_dir(&self.settings.backup_dir).await?;
        let mut backups: Vec<PathBuf> = Vec::new();
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                backups.push(path);
            }
        }
        
        backups.sort();
        Ok(backups)
    }

    /// Restore from a backup file
    pub async fn restore_backup(&self, backup_path: &Path) -> ConfigResult<AppConfig> {
        self.import(backup_path).await
    }
}

// Convenience methods for specific config sections
impl ConfigStore {
    /// Update monitored directories
    pub async fn set_monitored_directories(&self, dirs: Vec<PathBuf>) -> ConfigResult<AppConfig> {
        self.update(|config| {
            config.monitored_directories = dirs;
        }).await
    }

    /// Add a monitored directory
    pub async fn add_monitored_directory(&self, dir: PathBuf) -> ConfigResult<AppConfig> {
        self.update(|config| {
            if !config.monitored_directories.contains(&dir) {
                config.monitored_directories.push(dir);
            }
        }).await
    }

    /// Remove a monitored directory
    pub async fn remove_monitored_directory(&self, dir: &Path) -> ConfigResult<AppConfig> {
        self.update(|config| {
            config.monitored_directories.retain(|d| d != dir);
        }).await
    }

    /// Update cloud configuration
    pub async fn set_cloud_config(&self, cloud: CloudConfig) -> ConfigResult<AppConfig> {
        self.update(|config| {
            config.cloud = cloud;
        }).await
    }

    /// Enable/disable cloud features
    pub async fn set_cloud_enabled(&self, enabled: bool) -> ConfigResult<AppConfig> {
        self.update(|config| {
            config.cloud.enabled = enabled;
        }).await
    }

    /// Update UI theme
    pub async fn set_theme(&self, theme: String) -> ConfigResult<AppConfig> {
        self.update(|config| {
            config.ui.theme = theme;
        }).await
    }

    /// Update language
    pub async fn set_language(&self, language: String) -> ConfigResult<AppConfig> {
        self.update(|config| {
            config.ui.language = language;
        }).await
    }

    /// Update privacy mode
    pub async fn set_privacy_mode(&self, enabled: bool) -> ConfigResult<AppConfig> {
        self.update(|config| {
            config.privacy.privacy_mode = enabled;
            if enabled {
                // Disable cloud when privacy mode is on
                config.cloud.enabled = false;
            }
        }).await
    }
}
