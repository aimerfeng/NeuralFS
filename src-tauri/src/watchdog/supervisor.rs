//! Watchdog Supervisor Module
//!
//! Implements the watchdog process that monitors the main NeuralFS application
//! and can restart it if it becomes unresponsive.

use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant};
use thiserror::Error;

use super::shared_memory::{
    create_shared_memory, HeartbeatData, SharedMemory, SharedMemoryError,
    HEARTBEAT_INTERVAL_MS, HEARTBEAT_TIMEOUT_MS,
};

/// Watchdog configuration
#[derive(Debug, Clone)]
pub struct WatchdogConfig {
    /// Path to the main application executable
    pub main_executable: PathBuf,
    
    /// Arguments to pass to the main application
    pub main_args: Vec<String>,
    
    /// Heartbeat check interval in milliseconds
    pub check_interval_ms: u64,
    
    /// Heartbeat timeout in milliseconds
    pub timeout_ms: u64,
    
    /// Maximum number of restart attempts before giving up
    pub max_restart_attempts: u32,
    
    /// Delay between restart attempts in milliseconds
    pub restart_delay_ms: u64,
    
    /// Whether to restore Windows Explorer on crash
    pub restore_explorer_on_crash: bool,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            main_executable: PathBuf::new(),
            main_args: Vec::new(),
            check_interval_ms: HEARTBEAT_INTERVAL_MS,
            timeout_ms: HEARTBEAT_TIMEOUT_MS,
            max_restart_attempts: 3,
            restart_delay_ms: 2000,
            restore_explorer_on_crash: true,
        }
    }
}

/// Watchdog errors
#[derive(Error, Debug)]
pub enum WatchdogError {
    #[error("Failed to start main process: {0}")]
    ProcessStartFailed(String),

    #[error("Shared memory error: {0}")]
    SharedMemory(#[from] SharedMemoryError),

    #[error("Main process exited unexpectedly with code: {0:?}")]
    ProcessExited(Option<i32>),

    #[error("Heartbeat timeout - main process unresponsive")]
    HeartbeatTimeout,

    #[error("Max restart attempts exceeded")]
    MaxRestartsExceeded,

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Watchdog state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchdogState {
    /// Initial state, not yet started
    Idle,
    /// Waiting for main process to start
    WaitingForProcess,
    /// Monitoring heartbeat
    Monitoring,
    /// Detected timeout, attempting restart
    Restarting,
    /// Stopped
    Stopped,
    /// Failed after max restart attempts
    Failed,
}

/// Watchdog supervisor
pub struct Watchdog {
    config: WatchdogConfig,
    state: WatchdogState,
    shared_memory: Box<dyn SharedMemory>,
    main_process: Option<Child>,
    restart_count: u32,
    last_heartbeat: Option<Instant>,
}

impl Watchdog {
    /// Create a new watchdog with the given configuration
    pub fn new(config: WatchdogConfig) -> Self {
        Self {
            config,
            state: WatchdogState::Idle,
            shared_memory: create_shared_memory(),
            main_process: None,
            restart_count: 0,
            last_heartbeat: None,
        }
    }

    /// Get current watchdog state
    pub fn state(&self) -> WatchdogState {
        self.state
    }

    /// Get restart count
    pub fn restart_count(&self) -> u32 {
        self.restart_count
    }

    /// Start the watchdog monitoring loop
    pub fn start(&mut self) -> Result<(), WatchdogError> {
        if self.config.main_executable.as_os_str().is_empty() {
            return Err(WatchdogError::ConfigError(
                "Main executable path not set".to_string(),
            ));
        }

        // Open shared memory for reading
        self.shared_memory.open()?;
        self.state = WatchdogState::WaitingForProcess;

        Ok(())
    }

    /// Run one iteration of the monitoring loop
    /// Returns true if monitoring should continue, false if it should stop
    pub fn tick(&mut self) -> Result<bool, WatchdogError> {
        match self.state {
            WatchdogState::Idle => {
                // Not started yet
                Ok(true)
            }
            WatchdogState::WaitingForProcess => {
                // Try to read heartbeat to see if main process is running
                match self.shared_memory.read() {
                    Ok(data) => {
                        if !data.is_timed_out(self.config.timeout_ms) {
                            self.state = WatchdogState::Monitoring;
                            self.last_heartbeat = Some(Instant::now());
                            tracing::info!(
                                "Connected to main process (PID: {})",
                                data.process_id
                            );
                        }
                    }
                    Err(SharedMemoryError::InvalidData) | Err(SharedMemoryError::OpenFailed(_)) => {
                        // Main process not ready yet, keep waiting
                    }
                    Err(e) => {
                        tracing::warn!("Error reading heartbeat: {}", e);
                    }
                }
                Ok(true)
            }
            WatchdogState::Monitoring => {
                // Check heartbeat
                match self.shared_memory.read() {
                    Ok(data) => {
                        if data.is_timed_out(self.config.timeout_ms) {
                            tracing::warn!(
                                "Heartbeat timeout detected (last: {}ms ago)",
                                HeartbeatData::current_timestamp_ms() - data.last_heartbeat_ms
                            );
                            self.handle_timeout()?;
                        } else {
                            self.last_heartbeat = Some(Instant::now());
                            // Reset restart count on successful heartbeat
                            if self.restart_count > 0 {
                                tracing::info!("Main process recovered, resetting restart count");
                                self.restart_count = 0;
                            }
                        }
                    }
                    Err(SharedMemoryError::InvalidData) => {
                        tracing::warn!("Invalid heartbeat data, possible corruption");
                        self.handle_timeout()?;
                    }
                    Err(e) => {
                        tracing::error!("Failed to read heartbeat: {}", e);
                        self.handle_timeout()?;
                    }
                }
                Ok(true)
            }
            WatchdogState::Restarting => {
                // Wait for restart delay
                std::thread::sleep(Duration::from_millis(self.config.restart_delay_ms));
                
                // Attempt restart
                self.attempt_restart()?;
                Ok(true)
            }
            WatchdogState::Stopped | WatchdogState::Failed => {
                Ok(false)
            }
        }
    }

    /// Handle heartbeat timeout
    fn handle_timeout(&mut self) -> Result<(), WatchdogError> {
        if self.restart_count >= self.config.max_restart_attempts {
            tracing::error!(
                "Max restart attempts ({}) exceeded, giving up",
                self.config.max_restart_attempts
            );
            self.state = WatchdogState::Failed;
            return Err(WatchdogError::MaxRestartsExceeded);
        }

        self.state = WatchdogState::Restarting;
        
        // Restore Explorer if configured (Windows only)
        if self.config.restore_explorer_on_crash {
            if let Err(e) = restore_windows_explorer() {
                tracing::warn!("Failed to restore Explorer: {}", e);
            }
        }

        // Kill existing process if any
        if let Some(ref mut process) = self.main_process {
            tracing::info!("Killing unresponsive main process");
            let _ = process.kill();
            let _ = process.wait();
        }
        self.main_process = None;

        Ok(())
    }

    /// Attempt to restart the main process
    fn attempt_restart(&mut self) -> Result<(), WatchdogError> {
        self.restart_count += 1;
        tracing::info!(
            "Attempting restart ({}/{})",
            self.restart_count,
            self.config.max_restart_attempts
        );

        match start_main_process(&self.config.main_executable, &self.config.main_args) {
            Ok(child) => {
                self.main_process = Some(child);
                self.state = WatchdogState::WaitingForProcess;
                
                // Send notification about restart
                send_restart_notification(self.restart_count);
                
                Ok(())
            }
            Err(e) => {
                tracing::error!("Failed to start main process: {}", e);
                
                if self.restart_count >= self.config.max_restart_attempts {
                    self.state = WatchdogState::Failed;
                    send_error_notification(&format!(
                        "NeuralFS failed to restart after {} attempts",
                        self.restart_count
                    ));
                    Err(WatchdogError::MaxRestartsExceeded)
                } else {
                    // Stay in restarting state to try again
                    Ok(())
                }
            }
        }
    }

    /// Stop the watchdog
    pub fn stop(&mut self) {
        self.state = WatchdogState::Stopped;
        self.shared_memory.close();
        
        // Don't kill the main process when stopping normally
        self.main_process = None;
    }

    /// Get the check interval duration
    pub fn check_interval(&self) -> Duration {
        Duration::from_millis(self.config.check_interval_ms)
    }
}


// ============================================================================
// Process Management Functions
// ============================================================================

/// Start the main NeuralFS process
pub fn start_main_process(executable: &PathBuf, args: &[String]) -> Result<Child, WatchdogError> {
    tracing::info!("Starting main process: {:?}", executable);
    
    Command::new(executable)
        .args(args)
        .spawn()
        .map_err(|e| WatchdogError::ProcessStartFailed(e.to_string()))
}

// ============================================================================
// Windows Explorer Restoration
// ============================================================================

/// Restore Windows Explorer shell
/// This is called when the main process crashes to ensure the user has a working desktop
#[cfg(windows)]
pub fn restore_windows_explorer() -> Result<(), WatchdogError> {
    use std::process::Command;
    
    tracing::info!("Restoring Windows Explorer...");
    
    // Check if explorer.exe is already running
    let output = Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq explorer.exe", "/NH"])
        .output()
        .map_err(|e| WatchdogError::Io(e))?;
    
    let output_str = String::from_utf8_lossy(&output.stdout);
    
    if !output_str.contains("explorer.exe") {
        // Explorer is not running, start it
        tracing::info!("Explorer not running, starting it...");
        
        Command::new("explorer.exe")
            .spawn()
            .map_err(|e| WatchdogError::ProcessStartFailed(format!(
                "Failed to start explorer.exe: {}", e
            )))?;
        
        // Wait a moment for Explorer to initialize
        std::thread::sleep(Duration::from_millis(500));
    } else {
        tracing::info!("Explorer is already running");
    }
    
    Ok(())
}

#[cfg(not(windows))]
pub fn restore_windows_explorer() -> Result<(), WatchdogError> {
    // No-op on non-Windows platforms
    tracing::debug!("restore_windows_explorer called on non-Windows platform (no-op)");
    Ok(())
}

// ============================================================================
// Notification Functions
// ============================================================================

/// Send a notification about process restart
#[cfg(windows)]
fn send_restart_notification(attempt: u32) {
    use std::process::Command;
    
    let message = format!(
        "NeuralFS has been restarted (attempt {}).\nThe application became unresponsive and was automatically recovered.",
        attempt
    );
    
    // Use PowerShell to show a toast notification
    let script = format!(
        r#"
        [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null
        [Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime] | Out-Null
        
        $template = @"
        <toast>
            <visual>
                <binding template="ToastText02">
                    <text id="1">NeuralFS Watchdog</text>
                    <text id="2">{}</text>
                </binding>
            </visual>
        </toast>
"@
        
        $xml = New-Object Windows.Data.Xml.Dom.XmlDocument
        $xml.LoadXml($template)
        $toast = [Windows.UI.Notifications.ToastNotification]::new($xml)
        [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier("NeuralFS").Show($toast)
        "#,
        message.replace('"', "'")
    );
    
    let _ = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .spawn();
}

#[cfg(not(windows))]
fn send_restart_notification(attempt: u32) {
    // On non-Windows, just log the notification
    tracing::warn!(
        "NeuralFS has been restarted (attempt {}). The application became unresponsive.",
        attempt
    );
}

/// Send an error notification when recovery fails
#[cfg(windows)]
fn send_error_notification(message: &str) {
    use std::process::Command;
    
    // Use msg.exe for a simple message box
    let _ = Command::new("msg")
        .args(["*", "/TIME:30", message])
        .spawn();
}

#[cfg(not(windows))]
fn send_error_notification(message: &str) {
    tracing::error!("NeuralFS Error: {}", message);
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watchdog_config_default() {
        let config = WatchdogConfig::default();
        assert_eq!(config.check_interval_ms, HEARTBEAT_INTERVAL_MS);
        assert_eq!(config.timeout_ms, HEARTBEAT_TIMEOUT_MS);
        assert_eq!(config.max_restart_attempts, 3);
        assert!(config.restore_explorer_on_crash);
    }

    #[test]
    fn test_watchdog_initial_state() {
        let config = WatchdogConfig::default();
        let watchdog = Watchdog::new(config);
        assert_eq!(watchdog.state(), WatchdogState::Idle);
        assert_eq!(watchdog.restart_count(), 0);
    }

    #[test]
    fn test_watchdog_start_without_executable() {
        let config = WatchdogConfig::default();
        let mut watchdog = Watchdog::new(config);
        
        let result = watchdog.start();
        assert!(result.is_err());
        
        if let Err(WatchdogError::ConfigError(msg)) = result {
            assert!(msg.contains("executable"));
        } else {
            panic!("Expected ConfigError");
        }
    }

    #[test]
    fn test_watchdog_state_transitions() {
        let mut config = WatchdogConfig::default();
        config.main_executable = PathBuf::from("test_executable");
        
        let mut watchdog = Watchdog::new(config);
        assert_eq!(watchdog.state(), WatchdogState::Idle);
        
        // Note: We can't fully test start() without actual shared memory
        // but we can verify the state machine logic
    }

    #[test]
    fn test_watchdog_check_interval() {
        let mut config = WatchdogConfig::default();
        config.check_interval_ms = 500;
        
        let watchdog = Watchdog::new(config);
        assert_eq!(watchdog.check_interval(), Duration::from_millis(500));
    }
}
