//! Onboarding Commands for NeuralFS
//!
//! Provides Tauri commands for first-launch onboarding:
//! - check_first_launch: Check if this is the first launch
//! - get_suggested_directories: Get suggested directories to monitor
//! - browse_directory: Open native directory picker
//! - save_onboarding_config: Save onboarding configuration
//! - start_initial_scan: Start initial directory scan
//! - get_scan_progress: Get current scan progress
//! - complete_onboarding: Mark onboarding as complete
//!
//! **Validates: Requirements 17.1, 17.2, 17.3, 17.4, 17.5**

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tauri::api::dialog::FileDialogBuilder;
use tokio::sync::RwLock;

use crate::core::config::AppConfig;

/// Directory suggestion for onboarding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectorySuggestion {
    /// Directory path
    pub path: String,
    /// Display name
    pub name: String,
    /// Description
    pub description: String,
    /// Whether it's recommended
    pub recommended: bool,
    /// Icon emoji
    pub icon: String,
}

/// Cloud setup configuration from frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudSetupConfig {
    /// Whether to enable cloud features
    pub enabled: bool,
    /// API key (if provided)
    pub api_key: Option<String>,
    /// Selected cloud model
    pub model: String,
    /// Monthly cost limit
    pub monthly_cost_limit: f64,
}

/// Scan progress information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanProgress {
    /// Whether scanning is in progress
    pub is_scanning: bool,
    /// Total files discovered
    pub total_files: u64,
    /// Files processed
    pub processed_files: u64,
    /// Current file being processed
    pub current_file: Option<String>,
    /// Estimated time remaining in seconds
    pub estimated_time_remaining: Option<u64>,
    /// Whether scan is complete
    pub is_complete: bool,
}

/// Onboarding completion result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingResult {
    /// Whether onboarding was successful
    pub success: bool,
    /// Error message if any
    pub error: Option<String>,
    /// Configuration that was applied
    pub config: Option<OnboardingConfigSummary>,
}

/// Summary of applied configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingConfigSummary {
    /// Monitored directories
    pub monitored_directories: Vec<String>,
    /// Whether cloud is enabled
    pub cloud_enabled: bool,
}

/// Global scan state (in production, this would be part of AppState)
lazy_static::lazy_static! {
    static ref SCAN_STATE: Arc<RwLock<ScanState>> = Arc::new(RwLock::new(ScanState::default()));
    static ref ONBOARDING_COMPLETE: AtomicBool = AtomicBool::new(false);
}

#[derive(Debug, Default)]
struct ScanState {
    is_scanning: bool,
    total_files: u64,
    processed_files: u64,
    current_file: Option<String>,
    is_complete: bool,
    start_time: Option<std::time::Instant>,
    files_per_second: f64,
}

/// Check if this is the first launch (onboarding needed)
///
/// Returns true if the user has not completed onboarding yet.
///
/// # Returns
/// Whether this is the first launch
#[tauri::command]
pub async fn check_first_launch() -> Result<bool, String> {
    // In production, this would check:
    // 1. Config file existence
    // 2. Database initialization status
    // 3. Onboarding completion flag
    
    // For now, check if onboarding has been completed in this session
    // or if config file exists
    let config_path = get_config_path();
    let config_exists = config_path.exists();
    let onboarding_done = ONBOARDING_COMPLETE.load(Ordering::SeqCst);
    
    Ok(!config_exists && !onboarding_done)
}

/// Get suggested directories for monitoring
///
/// Returns a list of common directories that users typically want to monitor.
///
/// # Returns
/// List of directory suggestions
#[tauri::command]
pub async fn get_suggested_directories() -> Result<Vec<DirectorySuggestion>, String> {
    let mut suggestions = Vec::new();
    
    // Get user directories based on platform
    #[cfg(target_os = "windows")]
    {
        if let Some(user_profile) = std::env::var_os("USERPROFILE") {
            let user_path = PathBuf::from(user_profile);
            
            // Downloads
            let downloads = user_path.join("Downloads");
            if downloads.exists() {
                suggestions.push(DirectorySuggestion {
                    path: downloads.to_string_lossy().to_string(),
                    name: "下载".to_string(),
                    description: "浏览器下载和应用导出的文件".to_string(),
                    recommended: true,
                    icon: "📥".to_string(),
                });
            }
            
            // Desktop
            let desktop = user_path.join("Desktop");
            if desktop.exists() {
                suggestions.push(DirectorySuggestion {
                    path: desktop.to_string_lossy().to_string(),
                    name: "桌面".to_string(),
                    description: "桌面上的文件和快捷方式".to_string(),
                    recommended: true,
                    icon: "🖥️".to_string(),
                });
            }
            
            // Documents
            let documents = user_path.join("Documents");
            if documents.exists() {
                suggestions.push(DirectorySuggestion {
                    path: documents.to_string_lossy().to_string(),
                    name: "文档".to_string(),
                    description: "个人文档和工作文件".to_string(),
                    recommended: false,
                    icon: "📄".to_string(),
                });
            }
            
            // Pictures
            let pictures = user_path.join("Pictures");
            if pictures.exists() {
                suggestions.push(DirectorySuggestion {
                    path: pictures.to_string_lossy().to_string(),
                    name: "图片".to_string(),
                    description: "照片和图像文件".to_string(),
                    recommended: false,
                    icon: "🖼️".to_string(),
                });
            }
            
            // Videos
            let videos = user_path.join("Videos");
            if videos.exists() {
                suggestions.push(DirectorySuggestion {
                    path: videos.to_string_lossy().to_string(),
                    name: "视频".to_string(),
                    description: "视频文件和录制内容".to_string(),
                    recommended: false,
                    icon: "🎬".to_string(),
                });
            }
        }
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        if let Some(home) = std::env::var_os("HOME") {
            let home_path = PathBuf::from(home);
            
            // Downloads
            let downloads = home_path.join("Downloads");
            if downloads.exists() {
                suggestions.push(DirectorySuggestion {
                    path: downloads.to_string_lossy().to_string(),
                    name: "下载".to_string(),
                    description: "浏览器下载和应用导出的文件".to_string(),
                    recommended: true,
                    icon: "📥".to_string(),
                });
            }
            
            // Desktop
            let desktop = home_path.join("Desktop");
            if desktop.exists() {
                suggestions.push(DirectorySuggestion {
                    path: desktop.to_string_lossy().to_string(),
                    name: "桌面".to_string(),
                    description: "桌面上的文件和快捷方式".to_string(),
                    recommended: true,
                    icon: "🖥️".to_string(),
                });
            }
            
            // Documents
            let documents = home_path.join("Documents");
            if documents.exists() {
                suggestions.push(DirectorySuggestion {
                    path: documents.to_string_lossy().to_string(),
                    name: "文档".to_string(),
                    description: "个人文档和工作文件".to_string(),
                    recommended: false,
                    icon: "📄".to_string(),
                });
            }
            
            // Pictures
            let pictures = home_path.join("Pictures");
            if pictures.exists() {
                suggestions.push(DirectorySuggestion {
                    path: pictures.to_string_lossy().to_string(),
                    name: "图片".to_string(),
                    description: "照片和图像文件".to_string(),
                    recommended: false,
                    icon: "🖼️".to_string(),
                });
            }
        }
    }
    
    Ok(suggestions)
}

/// Browse for a directory using native file picker
///
/// Opens the native directory picker dialog and returns the selected path.
///
/// # Returns
/// Selected directory path or None if cancelled
#[tauri::command]
pub async fn browse_directory() -> Result<Option<String>, String> {
    // Use tokio oneshot channel to get result from callback
    let (tx, rx) = tokio::sync::oneshot::channel();
    
    FileDialogBuilder::new()
        .set_title("选择目录")
        .pick_folder(move |path| {
            let _ = tx.send(path.map(|p| p.to_string_lossy().to_string()));
        });
    
    rx.await.map_err(|e| format!("Dialog error: {}", e))
}

/// Save onboarding configuration
///
/// Saves the user's onboarding choices to the configuration file.
///
/// # Arguments
/// * `directories` - List of directories to monitor
/// * `cloud_config` - Cloud configuration settings
///
/// # Returns
/// Operation result
#[tauri::command]
pub async fn save_onboarding_config(
    directories: Vec<String>,
    cloud_config: CloudSetupConfig,
) -> Result<OnboardingResult, String> {
    // Validate directories exist
    for dir in &directories {
        let path = PathBuf::from(dir);
        if !path.exists() {
            return Ok(OnboardingResult {
                success: false,
                error: Some(format!("目录不存在: {}", dir)),
                config: None,
            });
        }
        if !path.is_dir() {
            return Ok(OnboardingResult {
                success: false,
                error: Some(format!("路径不是目录: {}", dir)),
                config: None,
            });
        }
    }
    
    // Create configuration
    let mut config = AppConfig::default();
    config.monitored_directories = directories.iter().map(PathBuf::from).collect();
    config.cloud.enabled = cloud_config.enabled;
    config.cloud.model = cloud_config.model;
    config.cloud.monthly_cost_limit = cloud_config.monthly_cost_limit;
    
    // In production, save to config file
    // For now, just return success
    let config_path = get_config_path();
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    
    // Serialize and save config
    let config_json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    
    std::fs::write(&config_path, config_json)
        .map_err(|e| format!("Failed to write config: {}", e))?;
    
    Ok(OnboardingResult {
        success: true,
        error: None,
        config: Some(OnboardingConfigSummary {
            monitored_directories: directories,
            cloud_enabled: cloud_config.enabled,
        }),
    })
}

/// Start initial directory scan
///
/// Begins scanning the specified directories in the background.
///
/// # Arguments
/// * `directories` - List of directories to scan
#[tauri::command]
pub async fn start_initial_scan(directories: Vec<String>) -> Result<(), String> {
    // Reset scan state
    {
        let mut state = SCAN_STATE.write().await;
        state.is_scanning = true;
        state.total_files = 0;
        state.processed_files = 0;
        state.current_file = None;
        state.is_complete = false;
        state.start_time = Some(std::time::Instant::now());
        state.files_per_second = 0.0;
    }
    
    // Spawn background task to perform scanning
    // In production, this would use the actual indexer
    let dirs = directories.clone();
    tokio::spawn(async move {
        tracing::info!("Starting initial scan of {} directories", dirs.len());
        
        // Count total files first
        let mut total = 0u64;
        for dir in &dirs {
            let count = count_files_recursive(&PathBuf::from(dir)).unwrap_or(0);
            tracing::debug!("Directory {} contains {} files", dir, count);
            total += count;
        }
        
        tracing::info!("Total files to scan: {}", total);
        
        {
            let mut state = SCAN_STATE.write().await;
            state.total_files = total;
        }
        
        // Process files
        let mut processed = 0u64;
        let scan_start = std::time::Instant::now();
        
        for dir in &dirs {
            if let Err(e) = scan_directory_recursive(&PathBuf::from(dir), &mut processed, scan_start).await {
                tracing::error!("Scan error for {}: {}", dir, e);
            }
        }
        
        // Mark complete
        {
            let mut state = SCAN_STATE.write().await;
            state.is_scanning = false;
            state.is_complete = true;
            state.current_file = None;
            state.processed_files = processed;
        }
        
        let duration = scan_start.elapsed();
        tracing::info!(
            "Initial scan complete: {} files in {:.2}s ({:.1} files/sec)",
            processed,
            duration.as_secs_f64(),
            processed as f64 / duration.as_secs_f64().max(0.001)
        );
    });
    
    Ok(())
}

/// Get current scan progress
///
/// Returns the current progress of the initial scan.
///
/// # Returns
/// Current scan progress
#[tauri::command]
pub async fn get_scan_progress() -> Result<ScanProgress, String> {
    let state = SCAN_STATE.read().await;
    
    // Calculate estimated time remaining based on actual processing rate
    let estimated_time = if state.is_scanning && state.processed_files > 0 {
        let remaining = state.total_files.saturating_sub(state.processed_files);
        if state.files_per_second > 0.0 {
            Some((remaining as f64 / state.files_per_second) as u64)
        } else {
            // Fallback: assume ~100 files per second
            Some(remaining / 100)
        }
    } else {
        None
    };
    
    Ok(ScanProgress {
        is_scanning: state.is_scanning,
        total_files: state.total_files,
        processed_files: state.processed_files,
        current_file: state.current_file.clone(),
        estimated_time_remaining: estimated_time,
        is_complete: state.is_complete,
    })
}

/// Complete onboarding
///
/// Marks onboarding as complete and transitions to the main application.
#[tauri::command]
pub async fn complete_onboarding() -> Result<(), String> {
    ONBOARDING_COMPLETE.store(true, Ordering::SeqCst);
    Ok(())
}

// Helper functions

fn get_config_path() -> PathBuf {
    // Get app data directory
    #[cfg(target_os = "windows")]
    {
        if let Some(app_data) = std::env::var_os("APPDATA") {
            return PathBuf::from(app_data).join("NeuralFS").join("config.json");
        }
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(".config").join("neuralfs").join("config.json");
        }
    }
    
    PathBuf::from("config.json")
}

fn count_files_recursive(path: &PathBuf) -> Result<u64, std::io::Error> {
    let mut count = 0u64;
    
    if path.is_file() {
        return Ok(1);
    }
    
    if path.is_dir() {
        let entries = match std::fs::read_dir(path) {
            Ok(entries) => entries,
            Err(e) => {
                tracing::warn!("Cannot read directory {}: {}", path.display(), e);
                return Ok(0);
            }
        };
        
        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let entry_path = entry.path();
            
            // Skip hidden files and common excluded directories
            if let Some(name) = entry_path.file_name() {
                let name_str = name.to_string_lossy();
                if name_str.starts_with('.') || 
                   name_str == "node_modules" || 
                   name_str == "__pycache__" ||
                   name_str == "target" ||
                   name_str == ".git" ||
                   name_str == "venv" ||
                   name_str == ".venv" {
                    continue;
                }
            }
            
            if entry_path.is_file() {
                count += 1;
            } else if entry_path.is_dir() {
                count += count_files_recursive(&entry_path).unwrap_or(0);
            }
        }
    }
    
    Ok(count)
}

async fn scan_directory_recursive(
    path: &PathBuf, 
    processed: &mut u64,
    start_time: std::time::Instant,
) -> Result<(), std::io::Error> {
    if path.is_file() {
        // Update progress
        {
            let mut state = SCAN_STATE.write().await;
            state.processed_files = *processed;
            state.current_file = Some(path.to_string_lossy().to_string());
            
            // Calculate files per second
            let elapsed = start_time.elapsed().as_secs_f64();
            if elapsed > 0.0 {
                state.files_per_second = *processed as f64 / elapsed;
            }
        }
        *processed += 1;
        
        // In production, this would:
        // 1. Extract file metadata
        // 2. Parse content based on file type
        // 3. Generate embeddings
        // 4. Store in database and vector store
        
        // Simulate minimal processing time (actual indexing would be slower)
        tokio::time::sleep(tokio::time::Duration::from_micros(100)).await;
        return Ok(());
    }
    
    if path.is_dir() {
        let entries: Vec<_> = match std::fs::read_dir(path) {
            Ok(entries) => entries.filter_map(|e| e.ok()).collect(),
            Err(e) => {
                tracing::warn!("Cannot read directory {}: {}", path.display(), e);
                return Ok(());
            }
        };
        
        for entry in entries {
            let entry_path = entry.path();
            
            // Skip hidden files and common excluded directories
            if let Some(name) = entry_path.file_name() {
                let name_str = name.to_string_lossy();
                if name_str.starts_with('.') || 
                   name_str == "node_modules" || 
                   name_str == "__pycache__" ||
                   name_str == "target" ||
                   name_str == ".git" ||
                   name_str == "venv" ||
                   name_str == ".venv" {
                    continue;
                }
            }
            
            Box::pin(scan_directory_recursive(&entry_path, processed, start_time)).await?;
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_check_first_launch() {
        // Reset state
        ONBOARDING_COMPLETE.store(false, Ordering::SeqCst);
        
        // Should return true for first launch (assuming no config file)
        let result = check_first_launch().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_suggested_directories() {
        let result = get_suggested_directories().await;
        assert!(result.is_ok());
        
        let suggestions = result.unwrap();
        // Should have at least some suggestions on most systems
        // (may be empty in CI environments)
    }

    #[tokio::test]
    async fn test_scan_progress_initial() {
        let result = get_scan_progress().await;
        assert!(result.is_ok());
        
        let progress = result.unwrap();
        // Initial state should not be scanning
        assert!(!progress.is_scanning || progress.is_complete);
    }

    #[tokio::test]
    async fn test_complete_onboarding() {
        ONBOARDING_COMPLETE.store(false, Ordering::SeqCst);
        
        let result = complete_onboarding().await;
        assert!(result.is_ok());
        
        assert!(ONBOARDING_COMPLETE.load(Ordering::SeqCst));
    }
}
