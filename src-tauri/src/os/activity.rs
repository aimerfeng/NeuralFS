//! System Activity Monitor for NeuralFS
//!
//! This module provides system activity monitoring for:
//! - Fullscreen application detection (games, video players)
//! - Presentation mode detection
//! - Do Not Disturb mode detection
//! - Low power mode detection
//!
//! When a fullscreen application is detected, NeuralFS can enter "game mode"
//! to reduce resource usage and avoid interfering with the user's activity.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

/// System state representing the current activity context
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SystemState {
    /// Normal operation - no special activity detected
    Normal,

    /// Fullscreen application is running (games, video players, etc.)
    FullscreenApp {
        /// Name of the fullscreen application (if available)
        app_name: Option<String>,
        /// Process ID of the fullscreen application
        process_id: Option<u32>,
    },

    /// Presentation mode is active (PowerPoint, etc.)
    PresentationMode,

    /// Do Not Disturb mode is enabled
    DoNotDisturb,

    /// Low power mode (battery saver)
    LowPower {
        /// Current battery percentage
        battery_percent: u8,
    },
}

impl Default for SystemState {
    fn default() -> Self {
        SystemState::Normal
    }
}

impl SystemState {
    /// Check if this state should trigger game mode
    pub fn should_enter_game_mode(&self) -> bool {
        matches!(
            self,
            SystemState::FullscreenApp { .. } | SystemState::PresentationMode
        )
    }

    /// Check if this state indicates resource constraints
    pub fn has_resource_constraints(&self) -> bool {
        matches!(
            self,
            SystemState::FullscreenApp { .. }
                | SystemState::PresentationMode
                | SystemState::LowPower { .. }
        )
    }

    /// Get a human-readable description of the state
    pub fn description(&self) -> String {
        match self {
            SystemState::Normal => "Normal operation".to_string(),
            SystemState::FullscreenApp { app_name, .. } => {
                match app_name {
                    Some(name) => format!("Fullscreen app: {}", name),
                    None => "Fullscreen application detected".to_string(),
                }
            }
            SystemState::PresentationMode => "Presentation mode".to_string(),
            SystemState::DoNotDisturb => "Do Not Disturb".to_string(),
            SystemState::LowPower { battery_percent } => {
                format!("Low power mode ({}% battery)", battery_percent)
            }
        }
    }
}

/// Configuration for the system activity monitor
#[derive(Debug, Clone)]
pub struct ActivityMonitorConfig {
    /// Interval between state checks
    pub check_interval: Duration,

    /// Battery percentage threshold for low power mode
    pub low_power_threshold: u8,

    /// Whether to detect fullscreen applications
    pub detect_fullscreen: bool,

    /// Whether to detect presentation mode
    pub detect_presentation: bool,

    /// Whether to detect low power mode
    pub detect_low_power: bool,

    /// Process names to exclude from fullscreen detection (e.g., our own app)
    pub excluded_processes: Vec<String>,
}

impl Default for ActivityMonitorConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(2),
            low_power_threshold: 20,
            detect_fullscreen: true,
            detect_presentation: true,
            detect_low_power: true,
            excluded_processes: vec![
                "neuralfs".to_string(),
                "explorer.exe".to_string(),
            ],
        }
    }
}

/// Callback type for state change notifications
pub type StateChangeCallback = Box<dyn Fn(SystemState, SystemState) + Send + Sync>;

/// System Activity Monitor
///
/// Monitors system state and detects conditions that should trigger
/// game mode or other resource-saving behaviors.
pub struct SystemActivityMonitor {
    /// Configuration
    config: ActivityMonitorConfig,

    /// Current detected state
    current_state: Arc<RwLock<SystemState>>,

    /// State change callback
    on_state_change: Arc<RwLock<Option<StateChangeCallback>>>,

    /// Shutdown flag
    shutdown: Arc<std::sync::atomic::AtomicBool>,
}

impl SystemActivityMonitor {
    /// Create a new system activity monitor with default configuration
    pub fn new() -> Self {
        Self::with_config(ActivityMonitorConfig::default())
    }

    /// Create a new system activity monitor with custom configuration
    pub fn with_config(config: ActivityMonitorConfig) -> Self {
        Self {
            config,
            current_state: Arc::new(RwLock::new(SystemState::Normal)),
            on_state_change: Arc::new(RwLock::new(None)),
            shutdown: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Set the state change callback
    pub async fn set_on_state_change(&self, callback: StateChangeCallback) {
        let mut cb = self.on_state_change.write().await;
        *cb = Some(callback);
    }

    /// Get the current system state
    pub async fn get_state(&self) -> SystemState {
        self.current_state.read().await.clone()
    }

    /// Check if currently in game mode (fullscreen or presentation)
    pub async fn is_in_game_mode(&self) -> bool {
        self.get_state().await.should_enter_game_mode()
    }

    /// Start the monitoring loop
    pub fn start(&self) -> tokio::task::JoinHandle<()> {
        let state = self.current_state.clone();
        let callback = self.on_state_change.clone();
        let config = self.config.clone();
        let shutdown = self.shutdown.clone();

        tokio::spawn(async move {
            tracing::info!("System activity monitor started");

            loop {
                if shutdown.load(std::sync::atomic::Ordering::SeqCst) {
                    tracing::info!("System activity monitor shutting down");
                    break;
                }

                // Detect current system state
                let new_state = Self::detect_system_state(&config).await;

                // Check if state changed
                let mut current = state.write().await;
                if *current != new_state {
                    let old_state = current.clone();
                    *current = new_state.clone();

                    tracing::info!(
                        "System state changed: {} -> {}",
                        old_state.description(),
                        new_state.description()
                    );

                    // Notify callback
                    drop(current); // Release lock before callback
                    let cb = callback.read().await;
                    if let Some(ref callback_fn) = *cb {
                        callback_fn(old_state, new_state);
                    }
                } else {
                    drop(current);
                }

                tokio::time::sleep(config.check_interval).await;
            }
        })
    }

    /// Stop the monitoring loop
    pub fn stop(&self) {
        self.shutdown.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Manually trigger a state check
    pub async fn check_now(&self) -> SystemState {
        let new_state = Self::detect_system_state(&self.config).await;

        let mut current = self.current_state.write().await;
        if *current != new_state {
            let old_state = current.clone();
            *current = new_state.clone();

            drop(current);
            let cb = self.on_state_change.read().await;
            if let Some(ref callback_fn) = *cb {
                callback_fn(old_state, new_state.clone());
            }
        }

        new_state
    }

    /// Detect the current system state
    #[cfg(windows)]
    async fn detect_system_state(config: &ActivityMonitorConfig) -> SystemState {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;

        // Run detection in a blocking task since it uses Win32 APIs
        let config = config.clone();
        tokio::task::spawn_blocking(move || {
            Self::detect_system_state_windows(&config)
        })
        .await
        .unwrap_or(SystemState::Normal)
    }

    #[cfg(windows)]
    fn detect_system_state_windows(config: &ActivityMonitorConfig) -> SystemState {
        use windows::Win32::Foundation::{HWND, RECT};
        use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
        use windows::Win32::System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        use windows::Win32::UI::Shell::{SHQueryUserNotificationState, QUERY_USER_NOTIFICATION_STATE};
        use windows::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, GetSystemMetrics, GetWindowRect, GetWindowThreadProcessId,
            SM_CXSCREEN, SM_CYSCREEN,
        };

        unsafe {
            // 1. Check user notification state (presentation mode, D3D fullscreen, etc.)
            if config.detect_presentation {
                let mut notification_state = QUERY_USER_NOTIFICATION_STATE::default();
                if SHQueryUserNotificationState(&mut notification_state).is_ok() {
                    // QUNS_PRESENTATION_MODE = 5
                    if notification_state.0 == 5 {
                        return SystemState::PresentationMode;
                    }
                    // QUNS_RUNNING_D3D_FULL_SCREEN = 3
                    if notification_state.0 == 3 {
                        return SystemState::FullscreenApp {
                            app_name: None,
                            process_id: None,
                        };
                    }
                    // QUNS_BUSY = 2
                    if notification_state.0 == 2 {
                        return SystemState::DoNotDisturb;
                    }
                }
            }

            // 2. Check if foreground window is fullscreen
            if config.detect_fullscreen {
                let foreground = GetForegroundWindow();
                if foreground != HWND::default() {
                    let mut rect = RECT::default();
                    if GetWindowRect(foreground, &mut rect).is_ok() {
                        let screen_width = GetSystemMetrics(SM_CXSCREEN);
                        let screen_height = GetSystemMetrics(SM_CYSCREEN);

                        // Check if window covers the entire screen
                        let is_fullscreen = rect.left <= 0
                            && rect.top <= 0
                            && rect.right >= screen_width
                            && rect.bottom >= screen_height;

                        if is_fullscreen {
                            // Get process information
                            let mut process_id: u32 = 0;
                            GetWindowThreadProcessId(foreground, Some(&mut process_id));

                            // Skip our own process
                            let current_pid = std::process::id();
                            if process_id != current_pid && process_id != 0 {
                                // Try to get process name
                                let app_name = Self::get_process_name(process_id);

                                // Check if process is in exclusion list
                                if let Some(ref name) = app_name {
                                    let name_lower = name.to_lowercase();
                                    if config.excluded_processes.iter().any(|p| name_lower.contains(&p.to_lowercase())) {
                                        // Skip excluded process
                                    } else {
                                        return SystemState::FullscreenApp {
                                            app_name,
                                            process_id: Some(process_id),
                                        };
                                    }
                                } else {
                                    return SystemState::FullscreenApp {
                                        app_name: None,
                                        process_id: Some(process_id),
                                    };
                                }
                            }
                        }
                    }
                }
            }

            // 3. Check power status
            if config.detect_low_power {
                let mut power_status = SYSTEM_POWER_STATUS::default();
                if GetSystemPowerStatus(&mut power_status).is_ok() {
                    // Check if on battery and below threshold
                    // ACLineStatus: 0 = offline, 1 = online
                    if power_status.ACLineStatus == 0
                        && power_status.BatteryLifePercent != 255 // 255 = unknown
                        && power_status.BatteryLifePercent < config.low_power_threshold
                    {
                        return SystemState::LowPower {
                            battery_percent: power_status.BatteryLifePercent,
                        };
                    }
                }
            }

            SystemState::Normal
        }
    }

    #[cfg(windows)]
    fn get_process_name(process_id: u32) -> Option<String> {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;
        use windows::Win32::System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
            PROCESS_QUERY_LIMITED_INFORMATION,
        };

        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id).ok()?;

            let mut buffer = vec![0u16; 1024];
            let mut size = buffer.len() as u32;

            if QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, windows::core::PWSTR(buffer.as_mut_ptr()), &mut size).is_ok() {
                let path = OsString::from_wide(&buffer[..size as usize]);
                let path_str = path.to_string_lossy();

                // Extract just the filename
                if let Some(name) = path_str.rsplit('\\').next() {
                    return Some(name.to_string());
                }
            }

            None
        }
    }

    /// Detect the current system state (non-Windows stub)
    #[cfg(not(windows))]
    async fn detect_system_state(_config: &ActivityMonitorConfig) -> SystemState {
        // On non-Windows platforms, always return Normal
        // This is a stub implementation for cross-platform compilation
        SystemState::Normal
    }
}

impl Default for SystemActivityMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_state_default() {
        let state = SystemState::default();
        assert_eq!(state, SystemState::Normal);
    }

    #[test]
    fn test_system_state_game_mode_detection() {
        // Normal should not trigger game mode
        assert!(!SystemState::Normal.should_enter_game_mode());

        // Fullscreen app should trigger game mode
        let fullscreen = SystemState::FullscreenApp {
            app_name: Some("game.exe".to_string()),
            process_id: Some(1234),
        };
        assert!(fullscreen.should_enter_game_mode());

        // Presentation mode should trigger game mode
        assert!(SystemState::PresentationMode.should_enter_game_mode());

        // Do Not Disturb should NOT trigger game mode
        assert!(!SystemState::DoNotDisturb.should_enter_game_mode());

        // Low power should NOT trigger game mode
        let low_power = SystemState::LowPower { battery_percent: 15 };
        assert!(!low_power.should_enter_game_mode());
    }

    #[test]
    fn test_system_state_resource_constraints() {
        // Normal has no constraints
        assert!(!SystemState::Normal.has_resource_constraints());

        // Fullscreen has constraints
        let fullscreen = SystemState::FullscreenApp {
            app_name: None,
            process_id: None,
        };
        assert!(fullscreen.has_resource_constraints());

        // Presentation has constraints
        assert!(SystemState::PresentationMode.has_resource_constraints());

        // Low power has constraints
        let low_power = SystemState::LowPower { battery_percent: 10 };
        assert!(low_power.has_resource_constraints());

        // Do Not Disturb does NOT have resource constraints
        assert!(!SystemState::DoNotDisturb.has_resource_constraints());
    }

    #[test]
    fn test_system_state_description() {
        assert_eq!(SystemState::Normal.description(), "Normal operation");

        let fullscreen = SystemState::FullscreenApp {
            app_name: Some("game.exe".to_string()),
            process_id: Some(1234),
        };
        assert!(fullscreen.description().contains("game.exe"));

        let fullscreen_unknown = SystemState::FullscreenApp {
            app_name: None,
            process_id: None,
        };
        assert!(fullscreen_unknown.description().contains("Fullscreen"));

        assert_eq!(SystemState::PresentationMode.description(), "Presentation mode");
        assert_eq!(SystemState::DoNotDisturb.description(), "Do Not Disturb");

        let low_power = SystemState::LowPower { battery_percent: 15 };
        assert!(low_power.description().contains("15%"));
    }

    #[test]
    fn test_activity_monitor_config_default() {
        let config = ActivityMonitorConfig::default();
        assert_eq!(config.check_interval, Duration::from_secs(2));
        assert_eq!(config.low_power_threshold, 20);
        assert!(config.detect_fullscreen);
        assert!(config.detect_presentation);
        assert!(config.detect_low_power);
        assert!(!config.excluded_processes.is_empty());
    }

    #[tokio::test]
    async fn test_activity_monitor_creation() {
        let monitor = SystemActivityMonitor::new();
        let state = monitor.get_state().await;
        assert_eq!(state, SystemState::Normal);
    }

    #[tokio::test]
    async fn test_activity_monitor_is_in_game_mode() {
        let monitor = SystemActivityMonitor::new();
        // Initially should not be in game mode
        assert!(!monitor.is_in_game_mode().await);
    }

    #[test]
    fn test_system_state_equality() {
        let state1 = SystemState::FullscreenApp {
            app_name: Some("test.exe".to_string()),
            process_id: Some(100),
        };
        let state2 = SystemState::FullscreenApp {
            app_name: Some("test.exe".to_string()),
            process_id: Some(100),
        };
        let state3 = SystemState::FullscreenApp {
            app_name: Some("other.exe".to_string()),
            process_id: Some(200),
        };

        assert_eq!(state1, state2);
        assert_ne!(state1, state3);
    }

    #[test]
    fn test_system_state_serialization() {
        let state = SystemState::FullscreenApp {
            app_name: Some("game.exe".to_string()),
            process_id: Some(1234),
        };

        let json = serde_json::to_string(&state).unwrap();
        let deserialized: SystemState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, deserialized);
    }
}


// ============================================================================
// Game Mode Policy
// ============================================================================

use crate::embeddings::VRAMManager;
use crate::indexer::ResilientBatchIndexer;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

/// Configuration for game mode policy
#[derive(Debug, Clone)]
pub struct GameModePolicyConfig {
    /// Whether to release VRAM when entering game mode
    pub release_vram: bool,

    /// Whether to pause indexing when entering game mode
    pub pause_indexing: bool,

    /// Whether to disable cloud requests when entering game mode
    pub disable_cloud: bool,

    /// Delay before entering game mode (to avoid flickering)
    pub enter_delay: Duration,

    /// Delay before exiting game mode (to avoid flickering)
    pub exit_delay: Duration,
}

impl Default for GameModePolicyConfig {
    fn default() -> Self {
        Self {
            release_vram: true,
            pause_indexing: true,
            disable_cloud: true,
            enter_delay: Duration::from_millis(500),
            exit_delay: Duration::from_millis(1000),
        }
    }
}

/// Game mode status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameModeStatus {
    /// Not in game mode
    Inactive,
    /// Transitioning to game mode
    Entering,
    /// In game mode
    Active,
    /// Transitioning out of game mode
    Exiting,
}

impl Default for GameModeStatus {
    fn default() -> Self {
        GameModeStatus::Inactive
    }
}

/// Game Mode Policy
///
/// Manages system behavior when a fullscreen application (game, video player, etc.)
/// is detected. This includes:
/// - Releasing VRAM by evicting loaded models
/// - Pausing background indexing
/// - Disabling cloud API requests
pub struct GameModePolicy {
    /// Configuration
    config: GameModePolicyConfig,

    /// Current game mode status
    status: Arc<RwLock<GameModeStatus>>,

    /// VRAM manager reference (optional)
    vram_manager: Option<Arc<VRAMManager>>,

    /// Indexer reference (optional)
    indexer: Option<Arc<ResilientBatchIndexer>>,

    /// Cloud enabled flag (shared with cloud bridge)
    cloud_enabled: Arc<AtomicBool>,

    /// Indexer paused flag
    indexer_paused: Arc<AtomicBool>,
}

impl GameModePolicy {
    /// Create a new game mode policy with default configuration
    pub fn new() -> Self {
        Self::with_config(GameModePolicyConfig::default())
    }

    /// Create a new game mode policy with custom configuration
    pub fn with_config(config: GameModePolicyConfig) -> Self {
        Self {
            config,
            status: Arc::new(RwLock::new(GameModeStatus::Inactive)),
            vram_manager: None,
            indexer: None,
            cloud_enabled: Arc::new(AtomicBool::new(true)),
            indexer_paused: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Set the VRAM manager
    pub fn set_vram_manager(&mut self, vram_manager: Arc<VRAMManager>) {
        self.vram_manager = Some(vram_manager);
    }

    /// Set the indexer
    pub fn set_indexer(&mut self, indexer: Arc<ResilientBatchIndexer>) {
        self.indexer = Some(indexer);
    }

    /// Get the cloud enabled flag (for sharing with CloudBridge)
    pub fn cloud_enabled_flag(&self) -> Arc<AtomicBool> {
        self.cloud_enabled.clone()
    }

    /// Get the indexer paused flag
    pub fn indexer_paused_flag(&self) -> Arc<AtomicBool> {
        self.indexer_paused.clone()
    }

    /// Get current game mode status
    pub async fn get_status(&self) -> GameModeStatus {
        *self.status.read().await
    }

    /// Check if currently in game mode
    pub async fn is_active(&self) -> bool {
        matches!(
            *self.status.read().await,
            GameModeStatus::Active | GameModeStatus::Entering
        )
    }

    /// Check if cloud requests are enabled
    pub fn is_cloud_enabled(&self) -> bool {
        self.cloud_enabled.load(AtomicOrdering::SeqCst)
    }

    /// Check if indexer is paused
    pub fn is_indexer_paused(&self) -> bool {
        self.indexer_paused.load(AtomicOrdering::SeqCst)
    }

    /// Enter game mode
    ///
    /// This will:
    /// 1. Evict all loaded models to free VRAM
    /// 2. Pause background indexing
    /// 3. Disable cloud API requests
    pub async fn enter_game_mode(&self) {
        // Check if already in game mode
        {
            let status = self.status.read().await;
            if *status == GameModeStatus::Active || *status == GameModeStatus::Entering {
                return;
            }
        }

        // Set status to entering
        {
            let mut status = self.status.write().await;
            *status = GameModeStatus::Entering;
        }

        tracing::info!("Entering game mode");

        // Wait for enter delay
        tokio::time::sleep(self.config.enter_delay).await;

        // 1. Release VRAM by evicting all models
        if self.config.release_vram {
            if let Some(ref vram_manager) = self.vram_manager {
                tracing::debug!("Evicting all models to free VRAM");
                vram_manager.evict_all_models().await;
            }
        }

        // 2. Pause indexing
        if self.config.pause_indexing {
            tracing::debug!("Pausing background indexing");
            self.indexer_paused.store(true, AtomicOrdering::SeqCst);
        }

        // 3. Disable cloud requests
        if self.config.disable_cloud {
            tracing::debug!("Disabling cloud API requests");
            self.cloud_enabled.store(false, AtomicOrdering::SeqCst);
        }

        // Set status to active
        {
            let mut status = self.status.write().await;
            *status = GameModeStatus::Active;
        }

        tracing::info!("Game mode active");
    }

    /// Exit game mode
    ///
    /// This will:
    /// 1. Resume background indexing
    /// 2. Re-enable cloud API requests
    /// 3. Optionally prewarm frequently used models
    pub async fn exit_game_mode(&self) {
        // Check if not in game mode
        {
            let status = self.status.read().await;
            if *status == GameModeStatus::Inactive || *status == GameModeStatus::Exiting {
                return;
            }
        }

        // Set status to exiting
        {
            let mut status = self.status.write().await;
            *status = GameModeStatus::Exiting;
        }

        tracing::info!("Exiting game mode");

        // Wait for exit delay
        tokio::time::sleep(self.config.exit_delay).await;

        // 1. Resume indexing
        if self.config.pause_indexing {
            tracing::debug!("Resuming background indexing");
            self.indexer_paused.store(false, AtomicOrdering::SeqCst);
        }

        // 2. Re-enable cloud requests
        if self.config.disable_cloud {
            tracing::debug!("Re-enabling cloud API requests");
            self.cloud_enabled.store(true, AtomicOrdering::SeqCst);
        }

        // 3. Prewarm models (optional, done in background)
        if self.config.release_vram {
            if let Some(ref vram_manager) = self.vram_manager {
                tracing::debug!("Prewarming frequently used models");
                let _ = vram_manager.prewarm_models().await;
            }
        }

        // Set status to inactive
        {
            let mut status = self.status.write().await;
            *status = GameModeStatus::Inactive;
        }

        tracing::info!("Game mode deactivated");
    }

    /// Handle system state change
    ///
    /// This is called by the SystemActivityMonitor when the system state changes.
    /// It automatically enters or exits game mode based on the new state.
    pub async fn handle_state_change(&self, _old_state: SystemState, new_state: SystemState) {
        if new_state.should_enter_game_mode() {
            self.enter_game_mode().await;
        } else {
            self.exit_game_mode().await;
        }
    }

    /// Create a state change callback for use with SystemActivityMonitor
    pub fn create_state_change_callback(policy: Arc<tokio::sync::RwLock<GameModePolicy>>) -> StateChangeCallback {
        Box::new(move |old_state, new_state| {
            let policy = policy.clone();
            tokio::spawn(async move {
                let policy = policy.read().await;
                policy.handle_state_change(old_state, new_state).await;
            });
        })
    }
}

impl Default for GameModePolicy {
    fn default() -> Self {
        Self::new()
    }
}

/// Integrated Game Mode Controller
///
/// Combines SystemActivityMonitor and GameModePolicy for easy setup.
pub struct GameModeController {
    /// Activity monitor
    monitor: SystemActivityMonitor,

    /// Game mode policy
    policy: Arc<tokio::sync::RwLock<GameModePolicy>>,

    /// Monitor task handle
    monitor_handle: Option<tokio::task::JoinHandle<()>>,
}

impl GameModeController {
    /// Create a new game mode controller
    pub fn new() -> Self {
        Self {
            monitor: SystemActivityMonitor::new(),
            policy: Arc::new(tokio::sync::RwLock::new(GameModePolicy::new())),
            monitor_handle: None,
        }
    }

    /// Create with custom configurations
    pub fn with_config(
        monitor_config: ActivityMonitorConfig,
        policy_config: GameModePolicyConfig,
    ) -> Self {
        Self {
            monitor: SystemActivityMonitor::with_config(monitor_config),
            policy: Arc::new(tokio::sync::RwLock::new(GameModePolicy::with_config(policy_config))),
            monitor_handle: None,
        }
    }

    /// Set the VRAM manager
    pub async fn set_vram_manager(&self, vram_manager: Arc<VRAMManager>) {
        let mut policy = self.policy.write().await;
        policy.set_vram_manager(vram_manager);
    }

    /// Set the indexer
    pub async fn set_indexer(&self, indexer: Arc<ResilientBatchIndexer>) {
        let mut policy = self.policy.write().await;
        policy.set_indexer(indexer);
    }

    /// Get the cloud enabled flag
    pub async fn cloud_enabled_flag(&self) -> Arc<AtomicBool> {
        let policy = self.policy.read().await;
        policy.cloud_enabled_flag()
    }

    /// Get the indexer paused flag
    pub async fn indexer_paused_flag(&self) -> Arc<AtomicBool> {
        let policy = self.policy.read().await;
        policy.indexer_paused_flag()
    }

    /// Start monitoring and automatic game mode management
    pub async fn start(&mut self) {
        // Set up the callback
        let policy = self.policy.clone();
        let callback = GameModePolicy::create_state_change_callback(policy);
        self.monitor.set_on_state_change(callback).await;

        // Start the monitor
        self.monitor_handle = Some(self.monitor.start());

        tracing::info!("Game mode controller started");
    }

    /// Stop monitoring
    pub fn stop(&mut self) {
        self.monitor.stop();
        if let Some(handle) = self.monitor_handle.take() {
            handle.abort();
        }
        tracing::info!("Game mode controller stopped");
    }

    /// Get current system state
    pub async fn get_system_state(&self) -> SystemState {
        self.monitor.get_state().await
    }

    /// Get current game mode status
    pub async fn get_game_mode_status(&self) -> GameModeStatus {
        let policy = self.policy.read().await;
        policy.get_status().await
    }

    /// Check if in game mode
    pub async fn is_in_game_mode(&self) -> bool {
        let policy = self.policy.read().await;
        policy.is_active().await
    }

    /// Manually enter game mode
    pub async fn enter_game_mode(&self) {
        let policy = self.policy.read().await;
        policy.enter_game_mode().await;
    }

    /// Manually exit game mode
    pub async fn exit_game_mode(&self) {
        let policy = self.policy.read().await;
        policy.exit_game_mode().await;
    }
}

impl Default for GameModeController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod game_mode_tests {
    use super::*;

    #[test]
    fn test_game_mode_policy_config_default() {
        let config = GameModePolicyConfig::default();
        assert!(config.release_vram);
        assert!(config.pause_indexing);
        assert!(config.disable_cloud);
        assert_eq!(config.enter_delay, Duration::from_millis(500));
        assert_eq!(config.exit_delay, Duration::from_millis(1000));
    }

    #[test]
    fn test_game_mode_status_default() {
        let status = GameModeStatus::default();
        assert_eq!(status, GameModeStatus::Inactive);
    }

    #[tokio::test]
    async fn test_game_mode_policy_creation() {
        let policy = GameModePolicy::new();
        assert_eq!(policy.get_status().await, GameModeStatus::Inactive);
        assert!(!policy.is_active().await);
        assert!(policy.is_cloud_enabled());
        assert!(!policy.is_indexer_paused());
    }

    #[tokio::test]
    async fn test_game_mode_policy_flags() {
        let policy = GameModePolicy::new();

        // Get flags
        let cloud_flag = policy.cloud_enabled_flag();
        let indexer_flag = policy.indexer_paused_flag();

        // Initially cloud is enabled and indexer is not paused
        assert!(cloud_flag.load(AtomicOrdering::SeqCst));
        assert!(!indexer_flag.load(AtomicOrdering::SeqCst));
    }

    #[tokio::test]
    async fn test_game_mode_controller_creation() {
        let controller = GameModeController::new();
        let state = controller.get_system_state().await;
        assert_eq!(state, SystemState::Normal);

        let status = controller.get_game_mode_status().await;
        assert_eq!(status, GameModeStatus::Inactive);
    }

    #[test]
    fn test_game_mode_status_serialization() {
        let status = GameModeStatus::Active;
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: GameModeStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, deserialized);
    }
}
