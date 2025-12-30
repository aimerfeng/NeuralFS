//! Stub Desktop Manager for non-Windows platforms
//!
//! Provides a no-op implementation for platforms where desktop takeover
//! is not supported (macOS, Linux).

use crate::core::error::{OsError, Result};
use crate::os::{DesktopManager, HotkeyCallback, MonitorInfo, MonitorRect, MultiMonitorStrategy};

/// Stub Desktop Manager for non-Windows platforms
pub struct StubDesktopManager {
    /// Simulated window handle
    hwnd: Option<usize>,
}

impl StubDesktopManager {
    /// Create a new stub desktop manager
    pub fn new() -> Self {
        Self { hwnd: None }
    }

    /// Create with a specific window handle
    pub fn with_hwnd(hwnd: usize) -> Self {
        Self { hwnd: Some(hwnd) }
    }

    /// Set the hotkey callback (no-op on non-Windows)
    pub fn set_hotkey_callback(&mut self, _callback: HotkeyCallback) {
        // No-op
    }
}

impl Default for StubDesktopManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopManager for StubDesktopManager {
    fn take_over_desktop(&mut self) -> Result<()> {
        Err(OsError::PlatformNotSupported {
            platform: std::env::consts::OS.to_string(),
        }.into())
    }

    fn release_desktop(&mut self) -> Result<()> {
        Ok(()) // No-op
    }

    fn is_desktop_taken_over(&self) -> bool {
        false
    }

    fn register_hotkey_hooks(&mut self) -> Result<()> {
        // No-op on non-Windows
        tracing::warn!("Hotkey hooks not supported on this platform");
        Ok(())
    }

    fn unregister_hotkey_hooks(&mut self) -> Result<()> {
        Ok(()) // No-op
    }

    fn hide_taskbar(&mut self) -> Result<()> {
        // No-op on non-Windows
        tracing::warn!("Taskbar control not supported on this platform");
        Ok(())
    }

    fn restore_taskbar(&mut self) -> Result<()> {
        Ok(()) // No-op
    }

    fn get_monitors(&self) -> Result<Vec<MonitorInfo>> {
        // Return a single default monitor
        Ok(vec![MonitorInfo {
            handle: 0,
            rect: MonitorRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            is_primary: true,
            dpi_scale: 1.0,
            name: "Default".to_string(),
        }])
    }

    fn setup_multi_monitor(&mut self, _strategy: MultiMonitorStrategy) -> Result<()> {
        // No-op on non-Windows
        Ok(())
    }

    fn handle_display_change(&mut self) -> Result<()> {
        // No-op on non-Windows
        Ok(())
    }

    fn update_window_handle(&mut self, hwnd: usize) -> Result<()> {
        self.hwnd = Some(hwnd);
        Ok(())
    }

    fn get_window_handle(&self) -> Option<usize> {
        self.hwnd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stub_manager_creation() {
        let manager = StubDesktopManager::new();
        assert!(!manager.is_desktop_taken_over());
    }

    #[test]
    fn test_stub_manager_take_over_fails() {
        let mut manager = StubDesktopManager::new();
        assert!(manager.take_over_desktop().is_err());
    }

    #[test]
    fn test_stub_manager_monitors() {
        let manager = StubDesktopManager::new();
        let monitors = manager.get_monitors().unwrap();
        assert_eq!(monitors.len(), 1);
        assert!(monitors[0].is_primary);
    }
}
