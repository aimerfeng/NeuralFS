//! Windows Taskbar Manager
//!
//! Provides functionality to hide and restore the Windows taskbar.

use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, ShowWindow, SW_HIDE, SW_SHOW};

use crate::core::error::{OsError, Result};

/// Taskbar Manager
///
/// Manages the Windows taskbar visibility.
pub struct TaskbarManager {
    /// Taskbar window handle
    taskbar_hwnd: Option<HWND>,
    /// Whether taskbar is currently hidden
    is_hidden: bool,
}

impl TaskbarManager {
    /// Create a new taskbar manager
    pub fn new() -> Self {
        Self {
            taskbar_hwnd: None,
            is_hidden: false,
        }
    }

    /// Find the taskbar window
    fn find_taskbar(&mut self) -> Result<HWND> {
        if let Some(hwnd) = self.taskbar_hwnd {
            return Ok(hwnd);
        }

        unsafe {
            // Shell_TrayWnd is the class name for the Windows taskbar
            let class_name: Vec<u16> = "Shell_TrayWnd\0".encode_utf16().collect();
            let taskbar = FindWindowW(PCWSTR::from_raw(class_name.as_ptr()), PCWSTR::null());

            if taskbar.0 == 0 {
                return Err(OsError::TaskbarControlFailed {
                    reason: "Could not find Shell_TrayWnd".to_string(),
                }.into());
            }

            self.taskbar_hwnd = Some(taskbar);
            Ok(taskbar)
        }
    }

    /// Hide the taskbar
    pub fn hide(&mut self) -> Result<()> {
        if self.is_hidden {
            return Ok(()); // Already hidden
        }

        let taskbar = self.find_taskbar()?;

        unsafe {
            ShowWindow(taskbar, SW_HIDE);
        }

        self.is_hidden = true;
        tracing::info!("Taskbar hidden");
        Ok(())
    }

    /// Restore the taskbar
    pub fn restore(&mut self) -> Result<()> {
        if !self.is_hidden {
            return Ok(()); // Not hidden
        }

        if let Some(taskbar) = self.taskbar_hwnd {
            unsafe {
                ShowWindow(taskbar, SW_SHOW);
            }
        }

        self.is_hidden = false;
        tracing::info!("Taskbar restored");
        Ok(())
    }

    /// Check if taskbar is hidden
    pub fn is_hidden(&self) -> bool {
        self.is_hidden
    }
}

impl Default for TaskbarManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TaskbarManager {
    fn drop(&mut self) {
        // Always restore taskbar on drop
        let _ = self.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_taskbar_manager_creation() {
        let manager = TaskbarManager::new();
        assert!(!manager.is_hidden());
    }

    #[test]
    fn test_taskbar_manager_default() {
        let manager = TaskbarManager::default();
        assert!(!manager.is_hidden());
    }
}
