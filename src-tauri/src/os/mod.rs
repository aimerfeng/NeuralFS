//! OS Integration Layer for NeuralFS
//!
//! This module provides platform-specific functionality for:
//! - Desktop takeover (WorkerW mounting on Windows)
//! - Keyboard hook interception
//! - Taskbar control
//! - Multi-monitor support
//! - Display change handling
//! - Window handle lifecycle management

#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub use windows::desktop::WindowsDesktopManager;

#[cfg(not(windows))]
pub mod stub;

#[cfg(not(windows))]
pub use stub::StubDesktopManager as WindowsDesktopManager;

#[cfg(test)]
mod tests;

use crate::core::error::{OsError, Result};

/// Monitor information
#[derive(Debug, Clone)]
pub struct MonitorInfo {
    /// Monitor handle (platform-specific)
    pub handle: usize,
    /// Monitor bounds (x, y, width, height)
    pub rect: MonitorRect,
    /// Whether this is the primary monitor
    pub is_primary: bool,
    /// DPI scale factor
    pub dpi_scale: f32,
    /// Monitor name/identifier
    pub name: String,
}

/// Monitor rectangle bounds
#[derive(Debug, Clone, Copy, Default)]
pub struct MonitorRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Multi-monitor rendering strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MultiMonitorStrategy {
    /// Primary monitor runs NeuralFS, others remain unchanged
    #[default]
    PrimaryOnly,
    /// All monitors unified rendering (spanning)
    Unified,
    /// Each monitor has independent NeuralFS instance
    Independent,
}

/// Desktop manager trait for cross-platform abstraction
pub trait DesktopManager: Send + Sync {
    /// Take over the desktop (mount to WorkerW on Windows)
    fn take_over_desktop(&mut self) -> Result<()>;

    /// Release desktop control and restore original state
    fn release_desktop(&mut self) -> Result<()>;

    /// Check if desktop is currently taken over
    fn is_desktop_taken_over(&self) -> bool;

    /// Register keyboard hooks for hotkey interception
    fn register_hotkey_hooks(&mut self) -> Result<()>;

    /// Unregister keyboard hooks
    fn unregister_hotkey_hooks(&mut self) -> Result<()>;

    /// Hide the system taskbar
    fn hide_taskbar(&mut self) -> Result<()>;

    /// Restore the system taskbar
    fn restore_taskbar(&mut self) -> Result<()>;

    /// Get all connected monitors
    fn get_monitors(&self) -> Result<Vec<MonitorInfo>>;

    /// Setup multi-monitor configuration
    fn setup_multi_monitor(&mut self, strategy: MultiMonitorStrategy) -> Result<()>;

    /// Handle display configuration change
    fn handle_display_change(&mut self) -> Result<()>;

    /// Update the main window handle (for Webview rebuilds)
    fn update_window_handle(&mut self, hwnd: usize) -> Result<()>;

    /// Get current window handle
    fn get_window_handle(&self) -> Option<usize>;
}

/// Hotkey event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    /// Win+D pressed (show desktop)
    WinD,
    /// Custom hotkey for NeuralFS toggle
    Toggle,
    /// Search activation hotkey
    Search,
}

/// Callback type for hotkey events
pub type HotkeyCallback = Box<dyn Fn(HotkeyEvent) + Send + Sync>;
