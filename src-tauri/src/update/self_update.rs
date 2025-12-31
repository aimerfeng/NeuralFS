//! Self-Update Module for NeuralFS
//!
//! Provides application self-update functionality with:
//! - Version checking against update server
//! - Secure download with checksum verification
//! - Atomic swap & restart mechanism
//! - Watchdog coordination for safe updates
//! - Rollback capability on failure

use chrono::{DateTime, Utc};
use reqwest::header::RETRY_AFTER;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::RwLock;

/// Error types for self-update operations
#[derive(Error, Debug)]
pub enum UpdateError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Failed to check for updates: {0}")]
    CheckFailed(String),

    #[error("Download failed: {0}")]
    DownloadFailed(String),

    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    #[error("Update script creation failed: {0}")]
    ScriptCreationFailed(String),

    #[error("Watchdog communication failed: {0}")]
    WatchdogError(String),

    #[error("No backup available for rollback")]
    NoBackupAvailable,

    #[error("Rate limited, retry after {retry_after_secs} seconds")]
    RateLimited { retry_after_secs: u64 },

    #[error("Invalid version format: {0}")]
    InvalidVersion(String),

    #[error("Update cancelled")]
    Cancelled,

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Result type for update operations
pub type Result<T> = std::result::Result<T, UpdateError>;


/// Semantic version representation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub prerelease: Option<String>,
}

impl Version {
    /// Create a new version
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
            prerelease: None,
        }
    }

    /// Create a version with prerelease tag
    pub fn with_prerelease(major: u32, minor: u32, patch: u32, prerelease: impl Into<String>) -> Self {
        Self {
            major,
            minor,
            patch,
            prerelease: Some(prerelease.into()),
        }
    }

    /// Parse version from string (e.g., "1.2.3" or "1.2.3-beta.1")
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim().trim_start_matches('v');
        
        let (version_part, prerelease) = if let Some(idx) = s.find('-') {
            (&s[..idx], Some(s[idx + 1..].to_string()))
        } else {
            (s, None)
        };

        let parts: Vec<&str> = version_part.split('.').collect();
        if parts.len() != 3 {
            return Err(UpdateError::InvalidVersion(format!(
                "Expected 3 version components, got {}",
                parts.len()
            )));
        }

        let major = parts[0]
            .parse()
            .map_err(|_| UpdateError::InvalidVersion(format!("Invalid major version: {}", parts[0])))?;
        let minor = parts[1]
            .parse()
            .map_err(|_| UpdateError::InvalidVersion(format!("Invalid minor version: {}", parts[1])))?;
        let patch = parts[2]
            .parse()
            .map_err(|_| UpdateError::InvalidVersion(format!("Invalid patch version: {}", parts[2])))?;

        Ok(Self {
            major,
            minor,
            patch,
            prerelease,
        })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref pre) = self.prerelease {
            write!(f, "{}.{}.{}-{}", self.major, self.minor, self.patch, pre)
        } else {
            write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
        }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.major.cmp(&other.major) {
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.minor.cmp(&other.minor) {
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.patch.cmp(&other.patch) {
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        // Prerelease versions are less than release versions
        match (&self.prerelease, &other.prerelease) {
            (None, None) => std::cmp::Ordering::Equal,
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(a), Some(b)) => a.cmp(b),
        }
    }
}


/// Information about an available update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    /// New version available
    pub version: Version,
    /// Release date
    pub release_date: DateTime<Utc>,
    /// Download URL for the update package
    pub download_url: String,
    /// Size of the update package in bytes
    pub size_bytes: u64,
    /// SHA256 checksum for verification
    pub sha256: String,
    /// Changelog/release notes
    pub changelog: String,
    /// Whether this is a critical security update
    pub is_critical: bool,
    /// Minimum version required to apply this update (for delta updates)
    pub min_version: Option<Version>,
}

/// Update download progress
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProgress {
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
    /// Current phase of the update
    pub phase: UpdatePhase,
}

/// Update phases
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum UpdatePhase {
    /// Checking for updates
    Checking,
    /// Downloading update package
    Downloading,
    /// Verifying checksum
    Verifying,
    /// Preparing to apply update
    Preparing,
    /// Applying update (swap & restart)
    Applying,
    /// Update complete
    Complete,
    /// Update failed
    Failed,
}

/// Update status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum UpdateStatus {
    /// No update available
    UpToDate,
    /// Update available
    Available,
    /// Update is being downloaded
    Downloading,
    /// Update downloaded and ready to apply
    Ready,
    /// Update is being applied
    Applying,
    /// Update failed
    Failed { reason: String },
}

/// Configuration for the self-updater
#[derive(Debug, Clone)]
pub struct SelfUpdaterConfig {
    /// Update server URL
    pub update_server: String,
    /// Directory for storing downloaded updates
    pub download_dir: PathBuf,
    /// HTTP request timeout
    pub timeout: Duration,
    /// Whether to check for updates automatically
    pub auto_check: bool,
    /// Interval between automatic update checks (in hours)
    pub check_interval_hours: u32,
    /// Whether to download updates automatically
    pub auto_download: bool,
    /// Channel for updates (stable, beta, nightly)
    pub channel: UpdateChannel,
}

/// Update channel
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum UpdateChannel {
    Stable,
    Beta,
    Nightly,
}

impl std::fmt::Display for UpdateChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateChannel::Stable => write!(f, "stable"),
            UpdateChannel::Beta => write!(f, "beta"),
            UpdateChannel::Nightly => write!(f, "nightly"),
        }
    }
}

impl Default for SelfUpdaterConfig {
    fn default() -> Self {
        let download_dir = directories::BaseDirs::new()
            .map(|dirs| dirs.data_local_dir().join("NeuralFS").join("updates"))
            .unwrap_or_else(|| PathBuf::from("updates"));

        Self {
            update_server: "https://updates.neuralfs.io".to_string(),
            download_dir,
            timeout: Duration::from_secs(60),
            auto_check: true,
            check_interval_hours: 24,
            auto_download: false,
            channel: UpdateChannel::Stable,
        }
    }
}


/// Progress callback type
pub type UpdateProgressCallback = Arc<dyn Fn(UpdateProgress) + Send + Sync>;

/// Watchdog command for update coordination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WatchdogCommand {
    /// Prepare for update (pause auto-restart)
    PrepareUpdate,
    /// Prepare for rollback
    PrepareRollback,
    /// Update complete, resume normal operation
    UpdateComplete,
    /// Normal shutdown
    Shutdown,
}

/// Watchdog IPC interface
pub struct WatchdogIpc {
    /// Named pipe name (Windows) or socket path (Unix)
    pipe_name: String,
}

impl WatchdogIpc {
    /// Create a new Watchdog IPC interface
    pub fn new(pipe_name: impl Into<String>) -> Self {
        Self {
            pipe_name: pipe_name.into(),
        }
    }

    /// Send a command to the watchdog
    #[cfg(windows)]
    pub async fn send(&self, cmd: WatchdogCommand) -> Result<()> {
        use tokio::net::windows::named_pipe::ClientOptions;
        use tokio::io::AsyncWriteExt;

        let pipe = ClientOptions::new()
            .open(&self.pipe_name)
            .map_err(|e| UpdateError::WatchdogError(format!("Failed to open pipe: {}", e)))?;

        let data = serde_json::to_vec(&cmd)?;
        
        // Write length prefix followed by data
        let len = data.len() as u32;
        let mut buffer = Vec::with_capacity(4 + data.len());
        buffer.extend_from_slice(&len.to_le_bytes());
        buffer.extend_from_slice(&data);

        pipe.try_write(&buffer)
            .map_err(|e| UpdateError::WatchdogError(format!("Failed to write to pipe: {}", e)))?;

        Ok(())
    }

    /// Send a command to the watchdog (non-Windows stub)
    #[cfg(not(windows))]
    pub async fn send(&self, cmd: WatchdogCommand) -> Result<()> {
        // On non-Windows platforms, log the command for testing
        tracing::info!("WatchdogIpc::send (stub): {:?}", cmd);
        Ok(())
    }
}

impl Default for WatchdogIpc {
    fn default() -> Self {
        Self::new(r"\\.\pipe\NeuralFS_Watchdog")
    }
}


/// Self-updater for NeuralFS application
pub struct SelfUpdater {
    /// Current application version
    current_version: Version,
    /// Configuration
    config: SelfUpdaterConfig,
    /// HTTP client
    client: reqwest::Client,
    /// Watchdog IPC
    watchdog_ipc: WatchdogIpc,
    /// Progress callback
    progress_callback: Option<UpdateProgressCallback>,
    /// Current update status
    status: Arc<RwLock<UpdateStatus>>,
    /// Cancellation flag
    cancelled: Arc<RwLock<bool>>,
    /// Downloaded update info (if any)
    pending_update: Arc<RwLock<Option<(UpdateInfo, PathBuf)>>>,
}

impl SelfUpdater {
    /// Create a new self-updater with the current version
    pub fn new(current_version: Version) -> Result<Self> {
        Self::with_config(current_version, SelfUpdaterConfig::default())
    }

    /// Create a new self-updater with custom configuration
    pub fn with_config(current_version: Version, config: SelfUpdaterConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .user_agent(format!("NeuralFS/{}", current_version))
            .build()?;

        Ok(Self {
            current_version,
            config,
            client,
            watchdog_ipc: WatchdogIpc::default(),
            progress_callback: None,
            status: Arc::new(RwLock::new(UpdateStatus::UpToDate)),
            cancelled: Arc::new(RwLock::new(false)),
            pending_update: Arc::new(RwLock::new(None)),
        })
    }

    /// Set progress callback
    pub fn with_progress_callback(mut self, callback: UpdateProgressCallback) -> Self {
        self.progress_callback = Some(callback);
        self
    }

    /// Set custom watchdog IPC
    pub fn with_watchdog_ipc(mut self, ipc: WatchdogIpc) -> Self {
        self.watchdog_ipc = ipc;
        self
    }

    /// Get current version
    pub fn current_version(&self) -> &Version {
        &self.current_version
    }

    /// Get current update status
    pub async fn status(&self) -> UpdateStatus {
        self.status.read().await.clone()
    }

    /// Set update status
    async fn set_status(&self, status: UpdateStatus) {
        *self.status.write().await = status;
    }

    /// Cancel ongoing update operation
    pub async fn cancel(&self) {
        *self.cancelled.write().await = true;
    }

    /// Reset cancellation flag
    pub async fn reset_cancel(&self) {
        *self.cancelled.write().await = false;
    }

    /// Check if operation is cancelled
    async fn is_cancelled(&self) -> bool {
        *self.cancelled.read().await
    }

    /// Report progress
    fn report_progress(&self, progress: UpdateProgress) {
        if let Some(callback) = &self.progress_callback {
            callback(progress);
        }
    }


    /// Check for available updates
    pub async fn check_for_updates(&self) -> Result<Option<UpdateInfo>> {
        self.report_progress(UpdateProgress {
            downloaded: 0,
            total: 0,
            percentage: 0,
            speed_bps: 0,
            eta_seconds: None,
            phase: UpdatePhase::Checking,
        });

        let url = format!(
            "{}/api/v1/updates/latest?version={}&channel={}&platform={}",
            self.config.update_server,
            self.current_version,
            self.config.channel,
            std::env::consts::OS
        );

        tracing::info!("Checking for updates at: {}", url);

        let response = self.client.get(&url).send().await?;

        // Handle rate limiting
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok())
                .unwrap_or(60);

            return Err(UpdateError::RateLimited {
                retry_after_secs: retry_after,
            });
        }

        if !response.status().is_success() {
            return Err(UpdateError::CheckFailed(format!(
                "Server returned status: {}",
                response.status()
            )));
        }

        // Parse response
        let update_info: Option<UpdateInfo> = response.json().await?;

        // Check if update is newer than current version
        if let Some(ref info) = update_info {
            if info.version > self.current_version {
                // Check minimum version requirement
                if let Some(ref min_ver) = info.min_version {
                    if self.current_version < *min_ver {
                        tracing::warn!(
                            "Update {} requires minimum version {}, current is {}",
                            info.version,
                            min_ver,
                            self.current_version
                        );
                        // Still return the update info, but caller should handle this
                    }
                }
                self.set_status(UpdateStatus::Available).await;
                return Ok(update_info);
            }
        }

        self.set_status(UpdateStatus::UpToDate).await;
        Ok(None)
    }

    /// Download an update
    pub async fn download_update(&self, info: &UpdateInfo) -> Result<PathBuf> {
        // Ensure download directory exists
        tokio::fs::create_dir_all(&self.config.download_dir).await?;

        let filename = format!("neuralfs-{}.update", info.version);
        let target_path = self.config.download_dir.join(&filename);
        let partial_path = self.config.download_dir.join(format!("{}.part", filename));

        self.set_status(UpdateStatus::Downloading).await;

        tracing::info!("Downloading update from: {}", info.download_url);

        // Check for existing partial download
        let mut downloaded: u64 = 0;
        if partial_path.exists() {
            let metadata = tokio::fs::metadata(&partial_path).await?;
            downloaded = metadata.len();
            tracing::info!("Resuming download from byte {}", downloaded);
        }

        // Build request with Range header for resume
        let mut request = self.client.get(&info.download_url);
        if downloaded > 0 {
            request = request.header("Range", format!("bytes={}-", downloaded));
        }

        let response = request.send().await?;

        if !response.status().is_success() && response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            return Err(UpdateError::DownloadFailed(format!(
                "Server returned status: {}",
                response.status()
            )));
        }

        // Open file for appending
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&partial_path)
            .await?;

        // Stream download
        let mut stream = response.bytes_stream();
        let start_time = std::time::Instant::now();
        let mut last_progress_time = start_time;

        use futures::StreamExt;
        while let Some(chunk_result) = stream.next().await {
            if self.is_cancelled().await {
                return Err(UpdateError::Cancelled);
            }

            let chunk = chunk_result?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;

            // Report progress (throttled)
            let now = std::time::Instant::now();
            if now.duration_since(last_progress_time) >= Duration::from_millis(100) {
                last_progress_time = now;

                let elapsed = now.duration_since(start_time).as_secs_f64();
                let speed_bps = if elapsed > 0.0 {
                    (downloaded as f64 / elapsed) as u64
                } else {
                    0
                };

                let eta_seconds = if speed_bps > 0 && downloaded < info.size_bytes {
                    Some((info.size_bytes - downloaded) / speed_bps)
                } else {
                    None
                };

                let percentage = if info.size_bytes > 0 {
                    ((downloaded as f64 / info.size_bytes as f64) * 100.0).min(100.0) as u8
                } else {
                    0
                };

                self.report_progress(UpdateProgress {
                    downloaded,
                    total: info.size_bytes,
                    percentage,
                    speed_bps,
                    eta_seconds,
                    phase: UpdatePhase::Downloading,
                });
            }
        }

        file.flush().await?;

        // Verify checksum
        self.report_progress(UpdateProgress {
            downloaded: info.size_bytes,
            total: info.size_bytes,
            percentage: 100,
            speed_bps: 0,
            eta_seconds: None,
            phase: UpdatePhase::Verifying,
        });

        let actual_checksum = self.calculate_checksum(&partial_path).await?;
        if actual_checksum != info.sha256 {
            tokio::fs::remove_file(&partial_path).await.ok();
            return Err(UpdateError::ChecksumMismatch {
                expected: info.sha256.clone(),
                actual: actual_checksum,
            });
        }

        // Rename to final path
        tokio::fs::rename(&partial_path, &target_path).await?;

        // Store pending update info
        *self.pending_update.write().await = Some((info.clone(), target_path.clone()));
        self.set_status(UpdateStatus::Ready).await;

        tracing::info!("Update downloaded successfully: {}", target_path.display());

        Ok(target_path)
    }

    /// Calculate SHA256 checksum of a file
    async fn calculate_checksum(&self, path: &Path) -> Result<String> {
        let mut file = File::open(path).await?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 1024 * 1024]; // 1MB buffer

        loop {
            let n = file.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }

        Ok(format!("{:x}", hasher.finalize()))
    }


    /// Apply a downloaded update (Swap & Restart)
    pub async fn apply_update(&self) -> Result<()> {
        let pending = self.pending_update.read().await;
        let (info, update_path) = pending
            .as_ref()
            .ok_or_else(|| UpdateError::DownloadFailed("No pending update".to_string()))?;

        self.report_progress(UpdateProgress {
            downloaded: info.size_bytes,
            total: info.size_bytes,
            percentage: 100,
            speed_bps: 0,
            eta_seconds: None,
            phase: UpdatePhase::Preparing,
        });

        let current_exe = std::env::current_exe()?;
        let backup_exe = current_exe.with_extension("old");

        tracing::info!("Applying update: {} -> {}", self.current_version, info.version);

        // 1. Notify Watchdog to prepare for update
        self.watchdog_ipc.send(WatchdogCommand::PrepareUpdate).await?;

        self.set_status(UpdateStatus::Applying).await;

        self.report_progress(UpdateProgress {
            downloaded: info.size_bytes,
            total: info.size_bytes,
            percentage: 100,
            speed_bps: 0,
            eta_seconds: None,
            phase: UpdatePhase::Applying,
        });

        // 2. Create update script (because current exe is locked)
        let update_script = self.create_update_script(
            &current_exe,
            update_path,
            &backup_exe,
        )?;

        tracing::info!("Created update script: {}", update_script.display());

        // 3. Launch update script
        #[cfg(windows)]
        {
            std::process::Command::new("cmd")
                .args(["/C", update_script.to_str().unwrap()])
                .spawn()
                .map_err(|e| UpdateError::ScriptCreationFailed(format!(
                    "Failed to launch update script: {}", e
                )))?;
        }

        #[cfg(not(windows))]
        {
            std::process::Command::new("sh")
                .arg(&update_script)
                .spawn()
                .map_err(|e| UpdateError::ScriptCreationFailed(format!(
                    "Failed to launch update script: {}", e
                )))?;
        }

        // 4. Exit current process (Watchdog will restart new version)
        tracing::info!("Exiting for update...");
        std::process::exit(0);
    }

    /// Create update script for atomic swap
    #[cfg(windows)]
    fn create_update_script(
        &self,
        current: &Path,
        new: &Path,
        backup: &Path,
    ) -> Result<PathBuf> {
        let script_path = self.config.download_dir.join("update.bat");

        let script = format!(
            r#"@echo off
:: NeuralFS Update Script
:: Wait for main process to exit
timeout /t 2 /nobreak > nul

:: Backup current version
if exist "{current}" (
    move /Y "{current}" "{backup}"
    if errorlevel 1 (
        echo Failed to backup current version
        exit /b 1
    )
)

:: Install new version
move /Y "{new}" "{current}"
if errorlevel 1 (
    echo Failed to install new version
    :: Attempt rollback
    if exist "{backup}" (
        move /Y "{backup}" "{current}"
    )
    exit /b 1
)

:: Start new version
start "" "{current}"

:: Cleanup
del "%~f0"
"#,
            current = current.display(),
            new = new.display(),
            backup = backup.display(),
        );

        std::fs::write(&script_path, script)
            .map_err(|e| UpdateError::ScriptCreationFailed(e.to_string()))?;

        Ok(script_path)
    }

    /// Create update script for atomic swap (Unix)
    #[cfg(not(windows))]
    fn create_update_script(
        &self,
        current: &Path,
        new: &Path,
        backup: &Path,
    ) -> Result<PathBuf> {
        let script_path = self.config.download_dir.join("update.sh");

        let script = format!(
            r#"#!/bin/bash
# NeuralFS Update Script
# Wait for main process to exit
sleep 2

# Backup current version
if [ -f "{current}" ]; then
    mv "{current}" "{backup}"
    if [ $? -ne 0 ]; then
        echo "Failed to backup current version"
        exit 1
    fi
fi

# Install new version
mv "{new}" "{current}"
if [ $? -ne 0 ]; then
    echo "Failed to install new version"
    # Attempt rollback
    if [ -f "{backup}" ]; then
        mv "{backup}" "{current}"
    fi
    exit 1
fi

# Make executable
chmod +x "{current}"

# Start new version
"{current}" &

# Cleanup
rm "$0"
"#,
            current = current.display(),
            new = new.display(),
            backup = backup.display(),
        );

        std::fs::write(&script_path, &script)
            .map_err(|e| UpdateError::ScriptCreationFailed(e.to_string()))?;

        // Make script executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script_path, perms)?;
        }

        Ok(script_path)
    }


    /// Rollback to previous version
    pub async fn rollback(&self) -> Result<()> {
        let current_exe = std::env::current_exe()?;
        let backup_exe = current_exe.with_extension("old");

        if !backup_exe.exists() {
            return Err(UpdateError::NoBackupAvailable);
        }

        tracing::info!("Rolling back to previous version...");

        // Notify Watchdog
        self.watchdog_ipc.send(WatchdogCommand::PrepareRollback).await?;

        // Create rollback script
        let rollback_script = self.create_rollback_script(&current_exe, &backup_exe)?;

        // Launch rollback script
        #[cfg(windows)]
        {
            std::process::Command::new("cmd")
                .args(["/C", rollback_script.to_str().unwrap()])
                .spawn()
                .map_err(|e| UpdateError::ScriptCreationFailed(format!(
                    "Failed to launch rollback script: {}", e
                )))?;
        }

        #[cfg(not(windows))]
        {
            std::process::Command::new("sh")
                .arg(&rollback_script)
                .spawn()
                .map_err(|e| UpdateError::ScriptCreationFailed(format!(
                    "Failed to launch rollback script: {}", e
                )))?;
        }

        std::process::exit(0);
    }

    /// Create rollback script
    #[cfg(windows)]
    fn create_rollback_script(&self, current: &Path, backup: &Path) -> Result<PathBuf> {
        let script_path = self.config.download_dir.join("rollback.bat");

        let script = format!(
            r#"@echo off
:: NeuralFS Rollback Script
:: Wait for main process to exit
timeout /t 2 /nobreak > nul

:: Delete current (failed) version
if exist "{current}" (
    del /F "{current}"
)

:: Restore backup
move /Y "{backup}" "{current}"
if errorlevel 1 (
    echo Failed to restore backup
    exit /b 1
)

:: Start restored version
start "" "{current}"

:: Cleanup
del "%~f0"
"#,
            current = current.display(),
            backup = backup.display(),
        );

        std::fs::write(&script_path, script)
            .map_err(|e| UpdateError::ScriptCreationFailed(e.to_string()))?;

        Ok(script_path)
    }

    /// Create rollback script (Unix)
    #[cfg(not(windows))]
    fn create_rollback_script(&self, current: &Path, backup: &Path) -> Result<PathBuf> {
        let script_path = self.config.download_dir.join("rollback.sh");

        let script = format!(
            r#"#!/bin/bash
# NeuralFS Rollback Script
# Wait for main process to exit
sleep 2

# Delete current (failed) version
rm -f "{current}"

# Restore backup
mv "{backup}" "{current}"
if [ $? -ne 0 ]; then
    echo "Failed to restore backup"
    exit 1
fi

# Make executable
chmod +x "{current}"

# Start restored version
"{current}" &

# Cleanup
rm "$0"
"#,
            current = current.display(),
            backup = backup.display(),
        );

        std::fs::write(&script_path, &script)
            .map_err(|e| UpdateError::ScriptCreationFailed(e.to_string()))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script_path, perms)?;
        }

        Ok(script_path)
    }

    /// Check if a backup exists for rollback
    pub fn has_backup(&self) -> bool {
        if let Ok(current_exe) = std::env::current_exe() {
            let backup_exe = current_exe.with_extension("old");
            backup_exe.exists()
        } else {
            false
        }
    }

    /// Clean up old backup files
    pub async fn cleanup_backup(&self) -> Result<()> {
        if let Ok(current_exe) = std::env::current_exe() {
            let backup_exe = current_exe.with_extension("old");
            if backup_exe.exists() {
                tokio::fs::remove_file(&backup_exe).await?;
                tracing::info!("Cleaned up backup: {}", backup_exe.display());
            }
        }
        Ok(())
    }

    /// Clean up downloaded update files
    pub async fn cleanup_downloads(&self) -> Result<()> {
        if self.config.download_dir.exists() {
            let mut entries = tokio::fs::read_dir(&self.config.download_dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.extension().map(|e| e == "update" || e == "part").unwrap_or(false) {
                    tokio::fs::remove_file(&path).await.ok();
                    tracing::info!("Cleaned up: {}", path.display());
                }
            }
        }
        Ok(())
    }

    /// Get pending update info if any
    pub async fn pending_update(&self) -> Option<UpdateInfo> {
        self.pending_update.read().await.as_ref().map(|(info, _)| info.clone())
    }
}

impl Default for SelfUpdater {
    fn default() -> Self {
        Self::new(Version::new(0, 1, 0)).expect("Failed to create default SelfUpdater")
    }
}


// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_new() {
        let v = Version::new(1, 2, 3);
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert!(v.prerelease.is_none());
    }

    #[test]
    fn test_version_with_prerelease() {
        let v = Version::with_prerelease(1, 0, 0, "beta.1");
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 0);
        assert_eq!(v.patch, 0);
        assert_eq!(v.prerelease, Some("beta.1".to_string()));
    }

    #[test]
    fn test_version_parse() {
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(v, Version::new(1, 2, 3));

        let v = Version::parse("v1.2.3").unwrap();
        assert_eq!(v, Version::new(1, 2, 3));

        let v = Version::parse("1.2.3-beta.1").unwrap();
        assert_eq!(v, Version::with_prerelease(1, 2, 3, "beta.1"));
    }

    #[test]
    fn test_version_parse_invalid() {
        assert!(Version::parse("1.2").is_err());
        assert!(Version::parse("1.2.3.4").is_err());
        assert!(Version::parse("a.b.c").is_err());
    }

    #[test]
    fn test_version_display() {
        let v = Version::new(1, 2, 3);
        assert_eq!(v.to_string(), "1.2.3");

        let v = Version::with_prerelease(1, 0, 0, "alpha");
        assert_eq!(v.to_string(), "1.0.0-alpha");
    }

    #[test]
    fn test_version_ordering() {
        assert!(Version::new(1, 0, 0) < Version::new(2, 0, 0));
        assert!(Version::new(1, 0, 0) < Version::new(1, 1, 0));
        assert!(Version::new(1, 0, 0) < Version::new(1, 0, 1));
        assert!(Version::new(1, 0, 0) == Version::new(1, 0, 0));

        // Prerelease is less than release
        assert!(Version::with_prerelease(1, 0, 0, "alpha") < Version::new(1, 0, 0));
        assert!(Version::with_prerelease(1, 0, 0, "alpha") < Version::with_prerelease(1, 0, 0, "beta"));
    }

    #[test]
    fn test_update_channel_display() {
        assert_eq!(UpdateChannel::Stable.to_string(), "stable");
        assert_eq!(UpdateChannel::Beta.to_string(), "beta");
        assert_eq!(UpdateChannel::Nightly.to_string(), "nightly");
    }

    #[test]
    fn test_default_config() {
        let config = SelfUpdaterConfig::default();
        assert!(!config.update_server.is_empty());
        assert!(config.auto_check);
        assert!(!config.auto_download);
        assert_eq!(config.channel, UpdateChannel::Stable);
    }

    #[test]
    fn test_update_phase_equality() {
        assert_eq!(UpdatePhase::Checking, UpdatePhase::Checking);
        assert_ne!(UpdatePhase::Checking, UpdatePhase::Downloading);
    }

    #[test]
    fn test_update_status_equality() {
        assert_eq!(UpdateStatus::UpToDate, UpdateStatus::UpToDate);
        assert_ne!(UpdateStatus::UpToDate, UpdateStatus::Available);
        
        let failed1 = UpdateStatus::Failed { reason: "test".to_string() };
        let failed2 = UpdateStatus::Failed { reason: "test".to_string() };
        assert_eq!(failed1, failed2);
    }

    #[test]
    fn test_watchdog_command_serialization() {
        let cmd = WatchdogCommand::PrepareUpdate;
        let json = serde_json::to_string(&cmd).unwrap();
        let deserialized: WatchdogCommand = serde_json::from_str(&json).unwrap();
        
        match deserialized {
            WatchdogCommand::PrepareUpdate => {}
            _ => panic!("Expected PrepareUpdate"),
        }
    }

    #[tokio::test]
    async fn test_self_updater_creation() {
        let updater = SelfUpdater::new(Version::new(1, 0, 0)).unwrap();
        assert_eq!(updater.current_version(), &Version::new(1, 0, 0));
        assert_eq!(updater.status().await, UpdateStatus::UpToDate);
    }

    #[tokio::test]
    async fn test_self_updater_cancel() {
        let updater = SelfUpdater::new(Version::new(1, 0, 0)).unwrap();
        
        assert!(!updater.is_cancelled().await);
        updater.cancel().await;
        assert!(updater.is_cancelled().await);
        updater.reset_cancel().await;
        assert!(!updater.is_cancelled().await);
    }

    #[tokio::test]
    async fn test_self_updater_status() {
        let updater = SelfUpdater::new(Version::new(1, 0, 0)).unwrap();
        
        assert_eq!(updater.status().await, UpdateStatus::UpToDate);
        updater.set_status(UpdateStatus::Available).await;
        assert_eq!(updater.status().await, UpdateStatus::Available);
    }

    #[test]
    fn test_has_backup() {
        let updater = SelfUpdater::new(Version::new(1, 0, 0)).unwrap();
        // In test environment, there's no backup
        // This just tests that the method doesn't panic
        let _ = updater.has_backup();
    }

    #[tokio::test]
    async fn test_pending_update_initially_none() {
        let updater = SelfUpdater::new(Version::new(1, 0, 0)).unwrap();
        assert!(updater.pending_update().await.is_none());
    }

    #[test]
    fn test_update_info_serialization() {
        let info = UpdateInfo {
            version: Version::new(2, 0, 0),
            release_date: Utc::now(),
            download_url: "https://example.com/update.zip".to_string(),
            size_bytes: 1000000,
            sha256: "abc123".to_string(),
            changelog: "Bug fixes".to_string(),
            is_critical: false,
            min_version: None,
        };

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: UpdateInfo = serde_json::from_str(&json).unwrap();
        
        assert_eq!(info.version, deserialized.version);
        assert_eq!(info.download_url, deserialized.download_url);
    }

    #[test]
    fn test_update_progress_serialization() {
        let progress = UpdateProgress {
            downloaded: 500,
            total: 1000,
            percentage: 50,
            speed_bps: 100000,
            eta_seconds: Some(5),
            phase: UpdatePhase::Downloading,
        };

        let json = serde_json::to_string(&progress).unwrap();
        let deserialized: UpdateProgress = serde_json::from_str(&json).unwrap();
        
        assert_eq!(progress.downloaded, deserialized.downloaded);
        assert_eq!(progress.percentage, deserialized.percentage);
        assert_eq!(progress.phase, deserialized.phase);
    }
}


// ============================================================================
// Update Coordinator - High-level update orchestration
// ============================================================================

/// Update coordinator for managing the full update lifecycle
pub struct UpdateCoordinator {
    updater: SelfUpdater,
}

impl UpdateCoordinator {
    /// Create a new update coordinator
    pub fn new(current_version: Version) -> Result<Self> {
        Ok(Self {
            updater: SelfUpdater::new(current_version)?,
        })
    }

    /// Create with custom configuration
    pub fn with_config(current_version: Version, config: SelfUpdaterConfig) -> Result<Self> {
        Ok(Self {
            updater: SelfUpdater::with_config(current_version, config)?,
        })
    }

    /// Get the underlying updater
    pub fn updater(&self) -> &SelfUpdater {
        &self.updater
    }

    /// Get mutable reference to the underlying updater
    pub fn updater_mut(&mut self) -> &mut SelfUpdater {
        &mut self.updater
    }

    /// Check for updates and optionally download
    pub async fn check_and_download(&self) -> Result<Option<UpdateInfo>> {
        // Check for updates
        let update_info = self.updater.check_for_updates().await?;

        if let Some(ref info) = update_info {
            tracing::info!(
                "Update available: {} -> {} (critical: {})",
                self.updater.current_version(),
                info.version,
                info.is_critical
            );

            // Download the update
            self.updater.download_update(info).await?;
        }

        Ok(update_info)
    }

    /// Perform a full update cycle: check, download, and apply
    pub async fn perform_update(&self) -> Result<()> {
        // Check and download
        let update_info = self.check_and_download().await?;

        if update_info.is_some() {
            // Apply the update (this will exit the process)
            self.updater.apply_update().await?;
        }

        Ok(())
    }

    /// Schedule an update to be applied on next restart
    pub async fn schedule_update(&self) -> Result<Option<UpdateInfo>> {
        // Just check and download, don't apply
        self.check_and_download().await
    }

    /// Apply a previously downloaded update
    pub async fn apply_pending_update(&self) -> Result<()> {
        if self.updater.pending_update().await.is_some() {
            self.updater.apply_update().await
        } else {
            Err(UpdateError::DownloadFailed("No pending update".to_string()))
        }
    }

    /// Get current update status
    pub async fn status(&self) -> UpdateStatus {
        self.updater.status().await
    }

    /// Cancel any ongoing update operation
    pub async fn cancel(&self) {
        self.updater.cancel().await;
    }
}

// ============================================================================
// Update State Machine for Atomic Updates
// ============================================================================

/// State machine for tracking update progress
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateState {
    /// No update in progress
    Idle,
    /// Checking for updates
    Checking,
    /// Update available, waiting for user confirmation
    AwaitingConfirmation,
    /// Downloading update
    Downloading,
    /// Download complete, verifying
    Verifying,
    /// Ready to apply
    ReadyToApply,
    /// Applying update (swap in progress)
    Applying,
    /// Update complete, restart required
    RestartRequired,
    /// Update failed, can rollback
    Failed,
    /// Rolled back to previous version
    RolledBack,
}

impl UpdateState {
    /// Check if the state allows cancellation
    pub fn can_cancel(&self) -> bool {
        matches!(
            self,
            UpdateState::Checking
                | UpdateState::AwaitingConfirmation
                | UpdateState::Downloading
                | UpdateState::Verifying
                | UpdateState::ReadyToApply
        )
    }

    /// Check if the state allows rollback
    pub fn can_rollback(&self) -> bool {
        matches!(self, UpdateState::Failed | UpdateState::RestartRequired)
    }

    /// Check if the state is terminal
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            UpdateState::Idle | UpdateState::RolledBack | UpdateState::RestartRequired
        )
    }
}

/// Represents the result of an atomic update operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomicUpdateResult {
    /// Whether the update was successful
    pub success: bool,
    /// The state after the operation
    pub final_state: UpdateState,
    /// Error message if failed
    pub error: Option<String>,
    /// Version before update
    pub from_version: Version,
    /// Version after update (if successful)
    pub to_version: Option<Version>,
}

impl AtomicUpdateResult {
    /// Create a successful result
    pub fn success(from: Version, to: Version) -> Self {
        Self {
            success: true,
            final_state: UpdateState::RestartRequired,
            error: None,
            from_version: from,
            to_version: Some(to),
        }
    }

    /// Create a failed result
    pub fn failure(from: Version, error: impl Into<String>) -> Self {
        Self {
            success: false,
            final_state: UpdateState::Failed,
            error: Some(error.into()),
            from_version: from,
            to_version: None,
        }
    }

    /// Create a rollback result
    pub fn rollback(from: Version) -> Self {
        Self {
            success: true,
            final_state: UpdateState::RolledBack,
            error: None,
            from_version: from.clone(),
            to_version: Some(from),
        }
    }
}
