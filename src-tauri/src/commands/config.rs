//! Config Commands for NeuralFS
//!
//! Provides Tauri commands for configuration management:
//! - get_config: Get current application configuration
//! - set_config: Update application configuration
//! - get_cloud_status: Get cloud service status
//! - set_cloud_enabled: Enable/disable cloud features
//!
//! **Validates: Requirements 15.1, 15.2, 15.7**

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;

use crate::config::{
    ConfigStore, ConfigStoreConfig, AppConfig, CloudConfig, 
    PerformanceConfig, PrivacyConfig, UIConfig,
};

/// Application state containing the config store
pub struct ConfigState {
    pub store: Arc<RwLock<Option<ConfigStore>>>,
}

impl ConfigState {
    pub fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(None)),
        }
    }

    /// Initialize the config store
    pub async fn initialize(&self) -> Result<(), String> {
        let settings = ConfigStoreConfig::default();
        let store = ConfigStore::new(settings)
            .await
            .map_err(|e| e.to_string())?;
        
        let mut guard = self.store.write().await;
        *guard = Some(store);
        Ok(())
    }

    /// Get the config store
    pub async fn get_store(&self) -> Result<ConfigStore, String> {
        let guard = self.store.read().await;
        guard.clone().ok_or_else(|| "Config store not initialized".to_string())
    }
}

impl Default for ConfigState {
    fn default() -> Self {
        Self::new()
    }
}

/// Application config DTO for frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfigDto {
    /// Configuration version
    pub version: u32,
    /// Monitored directories
    pub monitored_directories: Vec<String>,
    /// Cloud configuration
    pub cloud: CloudConfigDto,
    /// Performance configuration
    pub performance: PerformanceConfigDto,
    /// Privacy configuration
    pub privacy: PrivacyConfigDto,
    /// UI configuration
    pub ui: UIConfigDto,
    /// Last modified timestamp
    pub last_modified: String,
}

/// Cloud config DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudConfigDto {
    /// Whether cloud features are enabled
    pub enabled: bool,
    /// API endpoint URL
    pub endpoint: Option<String>,
    /// API key (masked for security)
    pub api_key_set: bool,
    /// Monthly cost limit in USD
    pub monthly_cost_limit: f64,
    /// Requests per minute limit
    pub requests_per_minute: u32,
    /// Cloud model to use
    pub model: String,
    /// Provider name
    pub provider: String,
}

/// Performance config DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfigDto {
    /// Maximum VRAM usage in MB
    pub max_vram_mb: u32,
    /// Number of indexing threads
    pub indexing_threads: u32,
    /// Batch size for embedding generation
    pub embedding_batch_size: u32,
    /// Enable CUDA acceleration
    pub enable_cuda: bool,
    /// Enable fast inference mode
    pub fast_inference_mode: bool,
}

/// Privacy config DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyConfigDto {
    /// Enable privacy mode (disables all cloud features)
    pub privacy_mode: bool,
    /// Directories to exclude from indexing
    pub excluded_directories: Vec<String>,
    /// File patterns to exclude
    pub excluded_patterns: Vec<String>,
    /// Enable telemetry
    pub enable_telemetry: bool,
}

/// UI config DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UIConfigDto {
    /// Theme (light/dark/system)
    pub theme: String,
    /// Language code
    pub language: String,
    /// Enable animations
    pub enable_animations: bool,
    /// Show file extensions
    pub show_extensions: bool,
    /// Default view mode
    pub default_view: String,
    /// Thumbnail size
    pub thumbnail_size: String,
}

/// Cloud status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudStatusDto {
    /// Whether cloud is enabled
    pub enabled: bool,
    /// Whether cloud is connected
    pub connected: bool,
    /// Current month's usage in USD
    pub current_month_usage: f64,
    /// Monthly cost limit
    pub monthly_limit: f64,
    /// Remaining budget
    pub remaining_budget: f64,
    /// API requests made this minute
    pub requests_this_minute: u32,
    /// Requests per minute limit
    pub requests_per_minute_limit: u32,
    /// Last successful API call timestamp
    pub last_api_call: Option<String>,
    /// Error message if any
    pub error: Option<String>,
}

/// Config update request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConfigRequest {
    /// Monitored directories (optional)
    pub monitored_directories: Option<Vec<String>>,
    /// Cloud configuration (optional)
    pub cloud: Option<UpdateCloudConfigRequest>,
    /// Performance configuration (optional)
    pub performance: Option<PerformanceConfigDto>,
    /// Privacy configuration (optional)
    pub privacy: Option<PrivacyConfigDto>,
    /// UI configuration (optional)
    pub ui: Option<UIConfigDto>,
}

/// Cloud config update request (includes API key)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCloudConfigRequest {
    /// Whether cloud features are enabled
    pub enabled: Option<bool>,
    /// API endpoint URL
    pub endpoint: Option<String>,
    /// API key (only set if provided)
    pub api_key: Option<String>,
    /// Monthly cost limit in USD
    pub monthly_cost_limit: Option<f64>,
    /// Requests per minute limit
    pub requests_per_minute: Option<u32>,
    /// Cloud model to use
    pub model: Option<String>,
    /// Provider name
    pub provider: Option<String>,
}

/// Config operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigOperationResult {
    /// Whether the operation succeeded
    pub success: bool,
    /// Human-readable message
    pub message: String,
    /// Updated configuration
    pub config: Option<AppConfigDto>,
}

/// Initialize config state
#[tauri::command]
pub async fn init_config(state: State<'_, ConfigState>) -> Result<(), String> {
    state.initialize().await
}

/// Get current application configuration
#[tauri::command]
pub async fn get_config(state: State<'_, ConfigState>) -> Result<AppConfigDto, String> {
    // Try to get from state, fall back to default if not initialized
    let store_guard = state.store.read().await;
    
    let config = if let Some(store) = store_guard.as_ref() {
        store.get().await
    } else {
        AppConfig::default()
    };
    
    Ok(app_config_to_dto(&config))
}

/// Update application configuration
#[tauri::command]
pub async fn set_config(
    state: State<'_, ConfigState>,
    request: UpdateConfigRequest,
) -> Result<ConfigOperationResult, String> {
    let store_guard = state.store.read().await;
    
    let Some(store) = store_guard.as_ref() else {
        return Ok(ConfigOperationResult {
            success: false,
            message: "Config store not initialized".to_string(),
            config: None,
        });
    };

    let updated = store.update(|config| {
        // Apply updates
        if let Some(dirs) = &request.monitored_directories {
            config.monitored_directories = dirs.iter().map(PathBuf::from).collect();
        }

        if let Some(cloud) = &request.cloud {
            if let Some(enabled) = cloud.enabled {
                config.cloud.enabled = enabled;
            }
            if let Some(endpoint) = &cloud.endpoint {
                config.cloud.endpoint = Some(endpoint.clone());
            }
            if let Some(api_key) = &cloud.api_key {
                config.cloud.api_key = Some(api_key.clone());
            }
            if let Some(limit) = cloud.monthly_cost_limit {
                config.cloud.monthly_cost_limit = limit;
            }
            if let Some(rpm) = cloud.requests_per_minute {
                config.cloud.requests_per_minute = rpm;
            }
            if let Some(model) = &cloud.model {
                config.cloud.model = model.clone();
            }
            if let Some(provider) = &cloud.provider {
                config.cloud.provider = provider.clone();
            }
        }

        if let Some(perf) = &request.performance {
            config.performance.max_vram_mb = perf.max_vram_mb;
            config.performance.indexing_threads = perf.indexing_threads;
            config.performance.embedding_batch_size = perf.embedding_batch_size;
            config.performance.enable_cuda = perf.enable_cuda;
            config.performance.fast_inference_mode = perf.fast_inference_mode;
        }

        if let Some(privacy) = &request.privacy {
            config.privacy.privacy_mode = privacy.privacy_mode;
            config.privacy.excluded_directories = privacy.excluded_directories
                .iter()
                .map(PathBuf::from)
                .collect();
            config.privacy.excluded_patterns = privacy.excluded_patterns.clone();
            config.privacy.enable_telemetry = privacy.enable_telemetry;
            
            // Privacy mode disables cloud
            if privacy.privacy_mode {
                config.cloud.enabled = false;
            }
        }

        if let Some(ui) = &request.ui {
            config.ui.theme = ui.theme.clone();
            config.ui.language = ui.language.clone();
            config.ui.enable_animations = ui.enable_animations;
            config.ui.show_extensions = ui.show_extensions;
            config.ui.default_view = ui.default_view.clone();
            config.ui.thumbnail_size = ui.thumbnail_size.clone();
        }
    }).await.map_err(|e| e.to_string())?;

    Ok(ConfigOperationResult {
        success: true,
        message: "Configuration updated successfully".to_string(),
        config: Some(app_config_to_dto(&updated)),
    })
}

/// Get cloud service status
#[tauri::command]
pub async fn get_cloud_status(state: State<'_, ConfigState>) -> Result<CloudStatusDto, String> {
    let store_guard = state.store.read().await;
    
    let config = if let Some(store) = store_guard.as_ref() {
        store.get().await
    } else {
        AppConfig::default()
    };

    // In production, this would query the CloudBridge for actual status
    Ok(CloudStatusDto {
        enabled: config.cloud.enabled,
        connected: config.cloud.enabled && config.cloud.api_key.is_some(),
        current_month_usage: 0.0, // Would come from CloudBridge
        monthly_limit: config.cloud.monthly_cost_limit,
        remaining_budget: config.cloud.monthly_cost_limit,
        requests_this_minute: 0,
        requests_per_minute_limit: config.cloud.requests_per_minute,
        last_api_call: None,
        error: None,
    })
}

/// Enable or disable cloud features
#[tauri::command]
pub async fn set_cloud_enabled(
    state: State<'_, ConfigState>,
    enabled: bool,
) -> Result<ConfigOperationResult, String> {
    let store_guard = state.store.read().await;
    
    let Some(store) = store_guard.as_ref() else {
        return Ok(ConfigOperationResult {
            success: false,
            message: "Config store not initialized".to_string(),
            config: None,
        });
    };

    let updated = store.set_cloud_enabled(enabled)
        .await
        .map_err(|e| e.to_string())?;

    let message = if enabled {
        "Cloud features enabled"
    } else {
        "Cloud features disabled"
    };

    Ok(ConfigOperationResult {
        success: true,
        message: message.to_string(),
        config: Some(app_config_to_dto(&updated)),
    })
}

/// Add a monitored directory
#[tauri::command]
pub async fn add_monitored_directory(
    state: State<'_, ConfigState>,
    path: String,
) -> Result<ConfigOperationResult, String> {
    // Validate path exists
    let path_buf = PathBuf::from(&path);
    if !path_buf.exists() {
        return Err(format!("Directory does not exist: {}", path));
    }
    if !path_buf.is_dir() {
        return Err(format!("Path is not a directory: {}", path));
    }

    let store_guard = state.store.read().await;
    
    let Some(store) = store_guard.as_ref() else {
        return Ok(ConfigOperationResult {
            success: false,
            message: "Config store not initialized".to_string(),
            config: None,
        });
    };

    let updated = store.add_monitored_directory(path_buf)
        .await
        .map_err(|e| e.to_string())?;

    Ok(ConfigOperationResult {
        success: true,
        message: "Directory added to monitoring".to_string(),
        config: Some(app_config_to_dto(&updated)),
    })
}

/// Remove a monitored directory
#[tauri::command]
pub async fn remove_monitored_directory(
    state: State<'_, ConfigState>,
    path: String,
) -> Result<ConfigOperationResult, String> {
    let path_buf = PathBuf::from(&path);

    let store_guard = state.store.read().await;
    
    let Some(store) = store_guard.as_ref() else {
        return Ok(ConfigOperationResult {
            success: false,
            message: "Config store not initialized".to_string(),
            config: None,
        });
    };

    let updated = store.remove_monitored_directory(&path_buf)
        .await
        .map_err(|e| e.to_string())?;

    Ok(ConfigOperationResult {
        success: true,
        message: "Directory removed from monitoring".to_string(),
        config: Some(app_config_to_dto(&updated)),
    })
}

/// Set UI theme
#[tauri::command]
pub async fn set_theme(
    state: State<'_, ConfigState>,
    theme: String,
) -> Result<ConfigOperationResult, String> {
    // Validate theme
    let valid_themes = ["light", "dark", "system"];
    if !valid_themes.contains(&theme.as_str()) {
        return Err(format!("Invalid theme: {}. Must be one of: {:?}", theme, valid_themes));
    }

    let store_guard = state.store.read().await;
    
    let Some(store) = store_guard.as_ref() else {
        return Ok(ConfigOperationResult {
            success: false,
            message: "Config store not initialized".to_string(),
            config: None,
        });
    };

    let updated = store.set_theme(theme.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(ConfigOperationResult {
        success: true,
        message: format!("Theme changed to {}", theme),
        config: Some(app_config_to_dto(&updated)),
    })
}

/// Set language
#[tauri::command]
pub async fn set_language(
    state: State<'_, ConfigState>,
    language: String,
) -> Result<ConfigOperationResult, String> {
    let store_guard = state.store.read().await;
    
    let Some(store) = store_guard.as_ref() else {
        return Ok(ConfigOperationResult {
            success: false,
            message: "Config store not initialized".to_string(),
            config: None,
        });
    };

    let updated = store.set_language(language.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(ConfigOperationResult {
        success: true,
        message: format!("Language changed to {}", language),
        config: Some(app_config_to_dto(&updated)),
    })
}

/// Export configuration to file
#[tauri::command]
pub async fn export_config(
    state: State<'_, ConfigState>,
    path: String,
) -> Result<ConfigOperationResult, String> {
    let path_buf = PathBuf::from(&path);

    let store_guard = state.store.read().await;
    
    let Some(store) = store_guard.as_ref() else {
        return Ok(ConfigOperationResult {
            success: false,
            message: "Config store not initialized".to_string(),
            config: None,
        });
    };

    store.export(&path_buf)
        .await
        .map_err(|e| e.to_string())?;

    Ok(ConfigOperationResult {
        success: true,
        message: format!("Configuration exported to {}", path),
        config: None,
    })
}

/// Import configuration from file
#[tauri::command]
pub async fn import_config(
    state: State<'_, ConfigState>,
    path: String,
) -> Result<ConfigOperationResult, String> {
    let path_buf = PathBuf::from(&path);

    if !path_buf.exists() {
        return Err(format!("File does not exist: {}", path));
    }

    let store_guard = state.store.read().await;
    
    let Some(store) = store_guard.as_ref() else {
        return Ok(ConfigOperationResult {
            success: false,
            message: "Config store not initialized".to_string(),
            config: None,
        });
    };

    let imported = store.import(&path_buf)
        .await
        .map_err(|e| e.to_string())?;

    Ok(ConfigOperationResult {
        success: true,
        message: "Configuration imported successfully".to_string(),
        config: Some(app_config_to_dto(&imported)),
    })
}

/// Reset configuration to defaults
#[tauri::command]
pub async fn reset_config(
    state: State<'_, ConfigState>,
) -> Result<ConfigOperationResult, String> {
    let store_guard = state.store.read().await;
    
    let Some(store) = store_guard.as_ref() else {
        return Ok(ConfigOperationResult {
            success: false,
            message: "Config store not initialized".to_string(),
            config: None,
        });
    };

    let reset = store.reset()
        .await
        .map_err(|e| e.to_string())?;

    Ok(ConfigOperationResult {
        success: true,
        message: "Configuration reset to defaults".to_string(),
        config: Some(app_config_to_dto(&reset)),
    })
}

/// List available backups
#[tauri::command]
pub async fn list_config_backups(
    state: State<'_, ConfigState>,
) -> Result<Vec<String>, String> {
    let store_guard = state.store.read().await;
    
    let Some(store) = store_guard.as_ref() else {
        return Ok(vec![]);
    };

    let backups = store.list_backups()
        .await
        .map_err(|e| e.to_string())?;

    Ok(backups.iter().map(|p| p.to_string_lossy().to_string()).collect())
}

/// Restore from backup
#[tauri::command]
pub async fn restore_config_backup(
    state: State<'_, ConfigState>,
    backup_path: String,
) -> Result<ConfigOperationResult, String> {
    let path_buf = PathBuf::from(&backup_path);

    if !path_buf.exists() {
        return Err(format!("Backup file does not exist: {}", backup_path));
    }

    let store_guard = state.store.read().await;
    
    let Some(store) = store_guard.as_ref() else {
        return Ok(ConfigOperationResult {
            success: false,
            message: "Config store not initialized".to_string(),
            config: None,
        });
    };

    let restored = store.restore_backup(&path_buf)
        .await
        .map_err(|e| e.to_string())?;

    Ok(ConfigOperationResult {
        success: true,
        message: "Configuration restored from backup".to_string(),
        config: Some(app_config_to_dto(&restored)),
    })
}

// Helper functions

fn app_config_to_dto(config: &AppConfig) -> AppConfigDto {
    AppConfigDto {
        version: config.version,
        monitored_directories: config.monitored_directories
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect(),
        cloud: cloud_config_to_dto(&config.cloud),
        performance: performance_config_to_dto(&config.performance),
        privacy: privacy_config_to_dto(&config.privacy),
        ui: ui_config_to_dto(&config.ui),
        last_modified: config.last_modified.clone(),
    }
}

fn cloud_config_to_dto(config: &CloudConfig) -> CloudConfigDto {
    CloudConfigDto {
        enabled: config.enabled,
        endpoint: config.endpoint.clone(),
        api_key_set: config.api_key.is_some(),
        monthly_cost_limit: config.monthly_cost_limit,
        requests_per_minute: config.requests_per_minute,
        model: config.model.clone(),
        provider: config.provider.clone(),
    }
}

fn performance_config_to_dto(config: &PerformanceConfig) -> PerformanceConfigDto {
    PerformanceConfigDto {
        max_vram_mb: config.max_vram_mb,
        indexing_threads: config.indexing_threads,
        embedding_batch_size: config.embedding_batch_size,
        enable_cuda: config.enable_cuda,
        fast_inference_mode: config.fast_inference_mode,
    }
}

fn privacy_config_to_dto(config: &PrivacyConfig) -> PrivacyConfigDto {
    PrivacyConfigDto {
        privacy_mode: config.privacy_mode,
        excluded_directories: config.excluded_directories
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect(),
        excluded_patterns: config.excluded_patterns.clone(),
        enable_telemetry: config.enable_telemetry,
    }
}

fn ui_config_to_dto(config: &UIConfig) -> UIConfigDto {
    UIConfigDto {
        theme: config.theme.clone(),
        language: config.language.clone(),
        enable_animations: config.enable_animations,
        show_extensions: config.show_extensions,
        default_view: config.default_view.clone(),
        thumbnail_size: config.thumbnail_size.clone(),
    }
}
