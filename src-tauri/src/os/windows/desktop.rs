//! Windows Desktop Manager - WorkerW mounting implementation
//!
//! This module implements the desktop takeover functionality by mounting
//! the NeuralFS window behind the desktop icons using the WorkerW technique.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, EnumWindows, FindWindowW, GetWindowLongW, SendMessageTimeoutW,
    SetParent, SetWindowLongW, SetWindowPos, ShowWindow, GWL_STYLE, HWND_TOP,
    SMTO_NORMAL, SW_SHOW, SWP_SHOWWINDOW, WS_CHILD, WS_POPUP,
};

use crate::core::error::{OsError, Result};
use crate::os::{DesktopManager, HotkeyCallback, HotkeyEvent, MonitorInfo, MultiMonitorStrategy};

use super::display_listener::{DisplayChangeEvent, DisplayChangeListener};
use super::handle_manager::WindowHandleManager;
use super::keyboard::KeyboardHookManager;
use super::monitor::MonitorManager;
use super::taskbar::TaskbarManager;

/// Windows Desktop Manager
///
/// Manages the desktop takeover process on Windows by:
/// 1. Finding the Progman window
/// 2. Sending a message to spawn WorkerW
/// 3. Mounting the NeuralFS window as a child of WorkerW
pub struct WindowsDesktopManager {
    /// Main window handle
    main_hwnd: Option<HWND>,
    /// WorkerW window handle (where we mount our window)
    worker_w_hwnd: Option<HWND>,
    /// Whether desktop is currently taken over
    is_taken_over: AtomicBool,
    /// Keyboard hook manager
    keyboard_manager: KeyboardHookManager,
    /// Taskbar manager
    taskbar_manager: TaskbarManager,
    /// Monitor manager
    monitor_manager: MonitorManager,
    /// Window handle manager
    handle_manager: WindowHandleManager,
    /// Display change listener
    display_listener: DisplayChangeListener,
    /// Current multi-monitor strategy
    monitor_strategy: MultiMonitorStrategy,
    /// Hotkey callback
    hotkey_callback: Option<Arc<HotkeyCallback>>,
}

impl WindowsDesktopManager {
    /// Create a new Windows Desktop Manager
    pub fn new() -> Self {
        Self {
            main_hwnd: None,
            worker_w_hwnd: None,
            is_taken_over: AtomicBool::new(false),
            keyboard_manager: KeyboardHookManager::new(),
            taskbar_manager: TaskbarManager::new(),
            monitor_manager: MonitorManager::new(),
            handle_manager: WindowHandleManager::new(),
            display_listener: DisplayChangeListener::new(),
            monitor_strategy: MultiMonitorStrategy::PrimaryOnly,
            hotkey_callback: None,
        }
    }

    /// Create with a specific window handle
    pub fn with_hwnd(hwnd: usize) -> Self {
        let mut manager = Self::new();
        manager.main_hwnd = Some(HWND(hwnd as isize));
        manager.handle_manager.set_current_hwnd(hwnd);
        manager
    }

    /// Set the hotkey callback
    pub fn set_hotkey_callback(&mut self, callback: HotkeyCallback) {
        self.hotkey_callback = Some(Arc::new(callback));
    }

    /// Start display change monitoring
    pub fn start_display_monitoring(&mut self) -> Result<()> {
        // Create a callback that will handle display changes
        let callback = Box::new(move |event: DisplayChangeEvent| {
            match event {
                DisplayChangeEvent::DisplayChange { width, height, bpp } => {
                    tracing::info!(
                        "Display configuration changed: {}x{} @ {} bpp",
                        width, height, bpp
                    );
                }
                DisplayChangeEvent::DeviceChange { change_type } => {
                    tracing::info!("Device change: {:?}", change_type);
                }
            }
        });

        self.display_listener.start(callback)
    }

    /// Stop display change monitoring
    pub fn stop_display_monitoring(&mut self) -> Result<()> {
        self.display_listener.stop()
    }

    /// Find the WorkerW window that sits behind desktop icons
    fn find_worker_w(&self) -> Result<HWND> {
        unsafe {
            // First, find Progman (Program Manager)
            let progman = FindWindowW(PCWSTR::from_raw("Progman\0".encode_utf16().collect::<Vec<_>>().as_ptr()), PCWSTR::null());
            
            if progman.0 == 0 {
                return Err(OsError::ProgmanNotFound.into());
            }

            // Send the magic message to spawn WorkerW
            // This is an undocumented Windows message (0x052C) that creates
            // a WorkerW window behind the desktop icons
            let mut _result: usize = 0;
            SendMessageTimeoutW(
                progman,
                0x052C,
                WPARAM(0),
                LPARAM(0),
                SMTO_NORMAL,
                1000,
                Some(&mut _result),
            );

            // Now enumerate windows to find the WorkerW
            let worker_w: AtomicUsize = AtomicUsize::new(0);
            
            let callback_data = &worker_w as *const AtomicUsize;
            
            EnumWindows(
                Some(Self::enum_windows_callback),
                LPARAM(callback_data as isize),
            )?;

            let hwnd_value = worker_w.load(Ordering::SeqCst);
            if hwnd_value == 0 {
                return Err(OsError::WorkerWNotFound.into());
            }

            Ok(HWND(hwnd_value as isize))
        }
    }

    /// Callback for EnumWindows to find WorkerW
    unsafe extern "system" fn enum_windows_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // Find SHELLDLL_DefView as a child of this window
        let shell_view = FindWindowW(
            PCWSTR::from_raw("SHELLDLL_DefView\0".encode_utf16().collect::<Vec<_>>().as_ptr()),
            PCWSTR::null(),
        );

        if shell_view.0 != 0 {
            // Found SHELLDLL_DefView, now find the WorkerW that's a sibling
            // The WorkerW we want is the one that comes after SHELLDLL_DefView's parent
            let worker_w = FindWindowW(
                PCWSTR::from_raw("WorkerW\0".encode_utf16().collect::<Vec<_>>().as_ptr()),
                PCWSTR::null(),
            );

            if worker_w.0 != 0 {
                let result_ptr = lparam.0 as *const AtomicUsize;
                if !result_ptr.is_null() {
                    (*result_ptr).store(worker_w.0 as usize, Ordering::SeqCst);
                }
                return BOOL(0); // Stop enumeration
            }
        }

        BOOL(1) // Continue enumeration
    }

    /// Get desktop dimensions
    fn get_desktop_size(&self) -> (i32, i32) {
        self.monitor_manager
            .get_primary_monitor()
            .map(|m| (m.rect.width, m.rect.height))
            .unwrap_or((1920, 1080))
    }

    /// Mount the window to WorkerW
    fn mount_to_worker_w(&mut self, worker_w: HWND) -> Result<()> {
        let main_hwnd = self.main_hwnd.ok_or_else(|| OsError::InvalidWindowHandle {
            reason: "Main window handle not set".to_string(),
        })?;

        unsafe {
            // Set our window as a child of WorkerW
            let result = SetParent(main_hwnd, worker_w);
            if result.0 == 0 {
                return Err(OsError::SetParentFailed {
                    reason: "SetParent returned null".to_string(),
                }.into());
            }

            // Modify window style to be a child window
            let style = GetWindowLongW(main_hwnd, GWL_STYLE);
            let new_style = (style as u32 & !WS_POPUP.0) | WS_CHILD.0;
            SetWindowLongW(main_hwnd, GWL_STYLE, new_style as i32);

            // Resize to cover the entire desktop
            let (width, height) = self.get_desktop_size();
            SetWindowPos(
                main_hwnd,
                HWND_TOP,
                0,
                0,
                width,
                height,
                SWP_SHOWWINDOW,
            )?;

            // Show the window
            ShowWindow(main_hwnd, SW_SHOW);
        }

        self.worker_w_hwnd = Some(worker_w);
        Ok(())
    }

    /// Unmount from WorkerW and restore original state
    fn unmount_from_worker_w(&mut self) -> Result<()> {
        let main_hwnd = match self.main_hwnd {
            Some(hwnd) => hwnd,
            None => return Ok(()), // Nothing to unmount
        };

        unsafe {
            // Remove parent (make it a top-level window again)
            SetParent(main_hwnd, HWND(0));

            // Restore window style
            let style = GetWindowLongW(main_hwnd, GWL_STYLE);
            let new_style = (style as u32 & !WS_CHILD.0) | WS_POPUP.0;
            SetWindowLongW(main_hwnd, GWL_STYLE, new_style as i32);
        }

        self.worker_w_hwnd = None;
        Ok(())
    }
}

impl Default for WindowsDesktopManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopManager for WindowsDesktopManager {
    fn take_over_desktop(&mut self) -> Result<()> {
        if self.is_taken_over.load(Ordering::SeqCst) {
            return Ok(()); // Already taken over
        }

        // Find WorkerW
        let worker_w = self.find_worker_w()?;

        // Mount our window
        self.mount_to_worker_w(worker_w)?;

        self.is_taken_over.store(true, Ordering::SeqCst);
        
        tracing::info!("Desktop takeover successful");
        Ok(())
    }

    fn release_desktop(&mut self) -> Result<()> {
        if !self.is_taken_over.load(Ordering::SeqCst) {
            return Ok(()); // Not taken over
        }

        // Unmount from WorkerW
        self.unmount_from_worker_w()?;

        // Restore taskbar if hidden
        self.restore_taskbar()?;

        // Unregister hooks
        self.unregister_hotkey_hooks()?;

        self.is_taken_over.store(false, Ordering::SeqCst);
        
        tracing::info!("Desktop released");
        Ok(())
    }

    fn is_desktop_taken_over(&self) -> bool {
        self.is_taken_over.load(Ordering::SeqCst)
    }

    fn register_hotkey_hooks(&mut self) -> Result<()> {
        self.keyboard_manager.register_hooks(self.hotkey_callback.clone())
    }

    fn unregister_hotkey_hooks(&mut self) -> Result<()> {
        self.keyboard_manager.unregister_hooks()
    }

    fn hide_taskbar(&mut self) -> Result<()> {
        self.taskbar_manager.hide()
    }

    fn restore_taskbar(&mut self) -> Result<()> {
        self.taskbar_manager.restore()
    }

    fn get_monitors(&self) -> Result<Vec<MonitorInfo>> {
        self.monitor_manager.enumerate_monitors()
    }

    fn setup_multi_monitor(&mut self, strategy: MultiMonitorStrategy) -> Result<()> {
        self.monitor_strategy = strategy;
        
        match strategy {
            MultiMonitorStrategy::PrimaryOnly => {
                // Resize to primary monitor only
                if let Some(primary) = self.monitor_manager.get_primary_monitor() {
                    if let Some(hwnd) = self.main_hwnd {
                        unsafe {
                            SetWindowPos(
                                hwnd,
                                HWND_TOP,
                                primary.rect.x,
                                primary.rect.y,
                                primary.rect.width,
                                primary.rect.height,
                                SWP_SHOWWINDOW,
                            )?;
                        }
                    }
                }
            }
            MultiMonitorStrategy::Unified => {
                // Span across all monitors
                let bounds = self.monitor_manager.get_virtual_screen_bounds();
                if let Some(hwnd) = self.main_hwnd {
                    unsafe {
                        SetWindowPos(
                            hwnd,
                            HWND_TOP,
                            bounds.x,
                            bounds.y,
                            bounds.width,
                            bounds.height,
                            SWP_SHOWWINDOW,
                        )?;
                    }
                }
            }
            MultiMonitorStrategy::Independent => {
                // This would require creating multiple windows
                // For now, just use primary
                tracing::warn!("Independent multi-monitor not yet implemented, using PrimaryOnly");
                return self.setup_multi_monitor(MultiMonitorStrategy::PrimaryOnly);
            }
        }
        
        Ok(())
    }

    fn handle_display_change(&mut self) -> Result<()> {
        // Refresh monitor information
        self.monitor_manager.refresh()?;

        // Re-apply current strategy
        let strategy = self.monitor_strategy;
        self.setup_multi_monitor(strategy)?;

        tracing::info!("Display change handled, monitors refreshed");
        Ok(())
    }

    fn update_window_handle(&mut self, hwnd: usize) -> Result<()> {
        let new_hwnd = HWND(hwnd as isize);
        let was_taken_over = self.is_taken_over.load(Ordering::SeqCst);

        // Update handle manager
        self.handle_manager.set_current_hwnd(hwnd);

        // If we were mounted, we need to remount with the new handle
        if was_taken_over {
            if let Some(worker_w) = self.worker_w_hwnd {
                self.main_hwnd = Some(new_hwnd);
                self.mount_to_worker_w(worker_w)?;
            }
        } else {
            self.main_hwnd = Some(new_hwnd);
        }

        tracing::debug!("Window handle updated to {:?}", hwnd);
        Ok(())
    }

    fn get_window_handle(&self) -> Option<usize> {
        self.main_hwnd.map(|h| h.0 as usize)
    }
}

impl Drop for WindowsDesktopManager {
    fn drop(&mut self) {
        // Ensure we clean up on drop
        let _ = self.release_desktop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_desktop_manager_creation() {
        let manager = WindowsDesktopManager::new();
        assert!(!manager.is_desktop_taken_over());
        assert!(manager.get_window_handle().is_none());
    }

    #[test]
    fn test_desktop_manager_with_hwnd() {
        let manager = WindowsDesktopManager::with_hwnd(12345);
        assert_eq!(manager.get_window_handle(), Some(12345));
    }

    #[test]
    fn test_multi_monitor_strategy_default() {
        let manager = WindowsDesktopManager::new();
        assert_eq!(manager.monitor_strategy, MultiMonitorStrategy::PrimaryOnly);
    }
}
