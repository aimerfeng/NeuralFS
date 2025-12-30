//! Windows Monitor Manager
//!
//! Provides multi-monitor enumeration and management functionality.

use std::sync::atomic::{AtomicUsize, Ordering};

use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW, MONITOR_DEFAULTTOPRIMARY,
    MonitorFromWindow,
};
use windows::Win32::UI::WindowsAndMessaging::GetDesktopWindow;

use crate::core::error::{OsError, Result};
use crate::os::{MonitorInfo, MonitorRect};

/// Monitor Manager
///
/// Manages monitor enumeration and provides information about connected displays.
pub struct MonitorManager {
    /// Cached monitor information
    monitors: Vec<MonitorInfo>,
    /// Whether cache is valid
    cache_valid: bool,
}

impl MonitorManager {
    /// Create a new monitor manager
    pub fn new() -> Self {
        Self {
            monitors: Vec::new(),
            cache_valid: false,
        }
    }

    /// Enumerate all connected monitors
    pub fn enumerate_monitors(&self) -> Result<Vec<MonitorInfo>> {
        let mut monitors: Vec<MonitorInfo> = Vec::new();
        let monitors_ptr = &mut monitors as *mut Vec<MonitorInfo>;

        unsafe {
            EnumDisplayMonitors(
                HDC::default(),
                None,
                Some(Self::monitor_enum_callback),
                LPARAM(monitors_ptr as isize),
            ).map_err(|e| OsError::MonitorEnumFailed {
                reason: format!("EnumDisplayMonitors failed: {:?}", e),
            })?;
        }

        Ok(monitors)
    }

    /// Refresh monitor cache
    pub fn refresh(&mut self) -> Result<()> {
        self.monitors = self.enumerate_monitors()?;
        self.cache_valid = true;
        tracing::debug!("Monitor cache refreshed, found {} monitors", self.monitors.len());
        Ok(())
    }

    /// Get cached monitors (refreshes if cache is invalid)
    pub fn get_monitors(&mut self) -> Result<&[MonitorInfo]> {
        if !self.cache_valid {
            self.refresh()?;
        }
        Ok(&self.monitors)
    }

    /// Get the primary monitor
    pub fn get_primary_monitor(&self) -> Option<&MonitorInfo> {
        // If cache is valid, use it
        if self.cache_valid {
            return self.monitors.iter().find(|m| m.is_primary);
        }

        // Otherwise, enumerate and find primary
        self.enumerate_monitors()
            .ok()
            .and_then(|monitors| monitors.into_iter().find(|m| m.is_primary))
            .as_ref()
            .map(|_| self.monitors.iter().find(|m| m.is_primary))
            .flatten()
    }

    /// Get virtual screen bounds (spanning all monitors)
    pub fn get_virtual_screen_bounds(&self) -> MonitorRect {
        let monitors = if self.cache_valid {
            &self.monitors
        } else {
            return MonitorRect::default();
        };

        if monitors.is_empty() {
            return MonitorRect::default();
        }

        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;

        for monitor in monitors {
            min_x = min_x.min(monitor.rect.x);
            min_y = min_y.min(monitor.rect.y);
            max_x = max_x.max(monitor.rect.x + monitor.rect.width);
            max_y = max_y.max(monitor.rect.y + monitor.rect.height);
        }

        MonitorRect {
            x: min_x,
            y: min_y,
            width: max_x - min_x,
            height: max_y - min_y,
        }
    }

    /// Monitor enumeration callback
    unsafe extern "system" fn monitor_enum_callback(
        hmonitor: HMONITOR,
        _hdc: HDC,
        _lprect: *mut RECT,
        lparam: LPARAM,
    ) -> BOOL {
        let monitors = &mut *(lparam.0 as *mut Vec<MonitorInfo>);

        // Get monitor info
        let mut monitor_info = MONITORINFOEXW::default();
        monitor_info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

        if GetMonitorInfoW(hmonitor, &mut monitor_info.monitorInfo).as_bool() {
            let rect = monitor_info.monitorInfo.rcMonitor;
            let is_primary = (monitor_info.monitorInfo.dwFlags & 1) != 0; // MONITORINFOF_PRIMARY

            // Get monitor name from szDevice
            let name = String::from_utf16_lossy(
                &monitor_info.szDevice[..monitor_info.szDevice.iter().position(|&c| c == 0).unwrap_or(monitor_info.szDevice.len())]
            );

            monitors.push(MonitorInfo {
                handle: hmonitor.0 as usize,
                rect: MonitorRect {
                    x: rect.left,
                    y: rect.top,
                    width: rect.right - rect.left,
                    height: rect.bottom - rect.top,
                },
                is_primary,
                dpi_scale: 1.0, // TODO: Get actual DPI scale
                name,
            });
        }

        BOOL(1) // Continue enumeration
    }

    /// Get monitor count
    pub fn monitor_count(&self) -> usize {
        if self.cache_valid {
            self.monitors.len()
        } else {
            self.enumerate_monitors().map(|m| m.len()).unwrap_or(0)
        }
    }
}

impl Default for MonitorManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monitor_manager_creation() {
        let manager = MonitorManager::new();
        assert!(!manager.cache_valid);
        assert!(manager.monitors.is_empty());
    }

    #[test]
    fn test_monitor_rect_default() {
        let rect = MonitorRect::default();
        assert_eq!(rect.x, 0);
        assert_eq!(rect.y, 0);
        assert_eq!(rect.width, 0);
        assert_eq!(rect.height, 0);
    }

    #[test]
    fn test_virtual_screen_bounds_empty() {
        let manager = MonitorManager::new();
        let bounds = manager.get_virtual_screen_bounds();
        assert_eq!(bounds.x, 0);
        assert_eq!(bounds.y, 0);
    }
}
