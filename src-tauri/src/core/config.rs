//! Configuration module for NeuralFS
//! 
//! Handles application configuration including:
//! - User preferences
//! - Cloud API settings
//! - Monitored directories
//! - Performance tuning

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Directories to monitor for file changes
    pub monitored_directories: Vec<PathBuf>,
    
    /// Cloud API configuration
    pub cloud: CloudConfig,
    
    /// Performance settings
    pub performance: PerformanceConfig,
    
    /// Privacy settings
    pub privacy: PrivacyConfig,
    
    /// UI preferences
    pub ui: UIConfig,
}

/// Cloud API configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudConfig {
    /// Whether cloud features are enabled
    pub enabled: bool,
    
    /// API endpoint URL
    pub endpoint: Option<String>,
    
    /// Monthly cost limit in USD
    pub monthly_cost_limit: f64,
    
    /// Requests per minute limit
    pub requests_per_minute: u32,
    
    /// Cloud model to use
    pub model: String,
}

/// Performance tuning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Maximum VRAM usage in MB
    pub max_vram_mb: u32,
    
    /// Number of indexing threads
    pub indexing_threads: u32,
    
    /// Batch size for embedding generation
    pub embedding_batch_size: u32,
    
    /// Enable CUDA acceleration
    pub enable_cuda: bool,
}

/// Privacy settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyConfig {
    /// Enable privacy mode (disables all cloud features)
    pub privacy_mode: bool,
    
    /// Directories to exclude from indexing
    pub excluded_directories: Vec<PathBuf>,
    
    /// File patterns to exclude
    pub excluded_patterns: Vec<String>,
}

/// UI preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UIConfig {
    /// Theme (light/dark)
    pub theme: String,
    
    /// Language code
    pub language: String,
    
    /// Enable animations
    pub enable_animations: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            monitored_directories: vec![],
            cloud: CloudConfig::default(),
            performance: PerformanceConfig::default(),
            privacy: PrivacyConfig::default(),
            ui: UIConfig::default(),
        }
    }
}

impl Default for CloudConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: None,
            monthly_cost_limit: 10.0,
            requests_per_minute: 60,
            model: "gpt-4o-mini".to_string(),
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            max_vram_mb: 4096, // 4GB default, leaving headroom on 6GB cards
            indexing_threads: 4,
            embedding_batch_size: 32,
            enable_cuda: true,
        }
    }
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            privacy_mode: false,
            excluded_directories: vec![],
            excluded_patterns: vec![
                "*.tmp".to_string(),
                "*.log".to_string(),
                "node_modules".to_string(),
                ".git".to_string(),
            ],
        }
    }
}

impl Default for UIConfig {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            language: "zh-CN".to_string(),
            enable_animations: true,
        }
    }
}
