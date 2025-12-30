//! Display Change Listener
//!
//! Monitors for display configuration changes (WM_DISPLAYCHANGE, WM_DEVICECHANGE)
//! and notifies the application to re-adjust window positioning.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    PostQuitMessage, RegisterClassW, TranslateMessage, CS_HREDRAW, CS_VREDRAW,
    CW_USEDEFAULT, MSG, WM_DESTROY, WM_DEVICECHANGE, WM_DISPLAYCHANGE, WNDCLASSW,
    WS_OVERLAPPEDWINDOW,
};

use crate::core::error::{OsError, Result};

/// Display change event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayChangeEvent {
    /// Display resolution or configuration changed
    DisplayChange {
        /// New width
        width: u32,
        /// New height
        height: u32,
        /// Bits per pixel
        bpp: u32,
    },
    /// Device added or removed
    DeviceChange {
        /// Device change type
        change_type: DeviceChangeType,
    },
}

/// Device change types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceChangeType {
    /// Device arrived
    Arrival,
    /// Device removed
    RemoveComplete,
    /// Configuration changed
    ConfigChanged,
    /// Other change
    Other(u32),
}

impl From<u32> for DeviceChangeType {
    fn from(value: u32) -> Self {
        match value {
            0x8000 => DeviceChangeType::Arrival,        // DBT_DEVICEARRIVAL
            0x8004 => DeviceChangeType::RemoveComplete, // DBT_DEVICEREMOVECOMPLETE
            0x0018 => DeviceChangeType::ConfigChanged,  // DBT_CONFIGCHANGED
            other => DeviceChangeType::Other(other),
        }
    }
}

/// Callback type for display change events
pub type DisplayChangeCallback = Box<dyn Fn(DisplayChangeEvent) + Send + Sync>;

/// Global callback storage
static mut DISPLAY_CALLBACK: Option<Arc<DisplayChangeCallback>> = None;

/// Global listener window handle
static LISTENER_HWND: AtomicUsize = AtomicUsize::new(0);

/// Global running flag
static LISTENER_RUNNING: AtomicBool = AtomicBool::new(false);

/// Display Change Listener
///
/// Creates a hidden window to receive display change notifications.
pub struct DisplayChangeListener {
    /// Whether the listener is running
    is_running: bool,
    /// Listener thread handle
    thread_handle: Option<thread::JoinHandle<()>>,
}

impl DisplayChangeListener {
    /// Create a new display change listener
    pub fn new() -> Self {
        Self {
            is_running: false,
            thread_handle: None,
        }
    }

    /// Start listening for display changes
    pub fn start(&mut self, callback: DisplayChangeCallback) -> Result<()> {
        if self.is_running {
            return Ok(()); // Already running
        }

        // Store callback globally
        unsafe {
            DISPLAY_CALLBACK = Some(Arc::new(callback));
        }

        LISTENER_RUNNING.store(true, Ordering::SeqCst);

        // Start listener thread
        let handle = thread::spawn(|| {
            if let Err(e) = Self::run_message_loop() {
                tracing::error!("Display listener error: {:?}", e);
            }
        });

        self.thread_handle = Some(handle);
        self.is_running = true;

        tracing::info!("Display change listener started");
        Ok(())
    }

    /// Stop listening for display changes
    pub fn stop(&mut self) -> Result<()> {
        if !self.is_running {
            return Ok(()); // Not running
        }

        LISTENER_RUNNING.store(false, Ordering::SeqCst);

        // Post quit message to the listener window
        let hwnd_value = LISTENER_HWND.load(Ordering::SeqCst);
        if hwnd_value != 0 {
            unsafe {
                let hwnd = HWND(hwnd_value as isize);
                DestroyWindow(hwnd).ok();
            }
        }

        // Wait for thread to finish
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }

        // Clear callback
        unsafe {
            DISPLAY_CALLBACK = None;
        }

        self.is_running = false;
        tracing::info!("Display change listener stopped");
        Ok(())
    }

    /// Check if listener is running
    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// Run the message loop (called from listener thread)
    fn run_message_loop() -> Result<()> {
        unsafe {
            // Register window class
            let class_name: Vec<u16> = "NeuralFS_DisplayListener\0".encode_utf16().collect();
            let class_name_ptr = PCWSTR::from_raw(class_name.as_ptr());

            let hinstance = GetModuleHandleW(None).map_err(|e| OsError::DisplayChangeFailed {
                reason: format!("GetModuleHandleW failed: {:?}", e),
            })?;

            let wc = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(Self::window_proc),
                hInstance: hinstance.into(),
                lpszClassName: class_name_ptr,
                ..Default::default()
            };

            let atom = RegisterClassW(&wc);
            if atom == 0 {
                return Err(OsError::DisplayChangeFailed {
                    reason: "RegisterClassW failed".to_string(),
                }.into());
            }

            // Create hidden message-only window
            let hwnd = CreateWindowExW(
                Default::default(),
                class_name_ptr,
                PCWSTR::null(),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                None,
                None,
                hinstance,
                None,
            );

            if hwnd.0 == 0 {
                return Err(OsError::DisplayChangeFailed {
                    reason: "CreateWindowExW failed".to_string(),
                }.into());
            }

            LISTENER_HWND.store(hwnd.0 as usize, Ordering::SeqCst);

            // Message loop
            let mut msg = MSG::default();
            while LISTENER_RUNNING.load(Ordering::SeqCst) {
                let result = GetMessageW(&mut msg, None, 0, 0);
                if result.0 <= 0 {
                    break;
                }

                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            LISTENER_HWND.store(0, Ordering::SeqCst);
        }

        Ok(())
    }

    /// Window procedure for handling display change messages
    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_DISPLAYCHANGE => {
                // Extract display info from lparam
                let width = (lparam.0 & 0xFFFF) as u32;
                let height = ((lparam.0 >> 16) & 0xFFFF) as u32;
                let bpp = wparam.0 as u32;

                let event = DisplayChangeEvent::DisplayChange { width, height, bpp };

                if let Some(ref callback) = DISPLAY_CALLBACK {
                    callback(event);
                }

                tracing::debug!(
                    "Display change detected: {}x{} @ {} bpp",
                    width,
                    height,
                    bpp
                );

                LRESULT(0)
            }
            WM_DEVICECHANGE => {
                let change_type = DeviceChangeType::from(wparam.0 as u32);
                let event = DisplayChangeEvent::DeviceChange { change_type };

                if let Some(ref callback) = DISPLAY_CALLBACK {
                    callback(event);
                }

                tracing::debug!("Device change detected: {:?}", change_type);

                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

impl Default for DisplayChangeListener {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DisplayChangeListener {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_listener_creation() {
        let listener = DisplayChangeListener::new();
        assert!(!listener.is_running());
    }

    #[test]
    fn test_device_change_type_conversion() {
        assert_eq!(DeviceChangeType::from(0x8000), DeviceChangeType::Arrival);
        assert_eq!(
            DeviceChangeType::from(0x8004),
            DeviceChangeType::RemoveComplete
        );
        assert_eq!(
            DeviceChangeType::from(0x0018),
            DeviceChangeType::ConfigChanged
        );
        assert_eq!(DeviceChangeType::from(0x1234), DeviceChangeType::Other(0x1234));
    }
}
