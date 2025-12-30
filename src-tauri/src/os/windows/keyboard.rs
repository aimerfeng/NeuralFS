//! Windows Keyboard Hook Manager
//!
//! Implements low-level keyboard hooks for intercepting system hotkeys
//! like Win+D and custom NeuralFS shortcuts.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_D, VK_LWIN, VK_RWIN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT,
    WH_KEYBOARD_LL,
};

use crate::core::error::{OsError, Result};
use crate::os::{HotkeyCallback, HotkeyEvent};

/// Global hook handle storage
static KEYBOARD_HOOK: AtomicUsize = AtomicUsize::new(0);

/// Global callback storage (we need this because the hook callback is extern "system")
static mut HOTKEY_CALLBACK: Option<Arc<Box<dyn Fn(HotkeyEvent) + Send + Sync>>> = None;

/// Keyboard Hook Manager
///
/// Manages low-level keyboard hooks for intercepting system hotkeys.
pub struct KeyboardHookManager {
    /// Whether hooks are currently registered
    is_registered: bool,
}

impl KeyboardHookManager {
    /// Create a new keyboard hook manager
    pub fn new() -> Self {
        Self {
            is_registered: false,
        }
    }

    /// Register keyboard hooks
    pub fn register_hooks(&mut self, callback: Option<Arc<HotkeyCallback>>) -> Result<()> {
        if self.is_registered {
            return Ok(()); // Already registered
        }

        // Store callback globally (unsafe but necessary for extern callback)
        if let Some(cb) = callback {
            unsafe {
                HOTKEY_CALLBACK = Some(Arc::new(cb));
            }
        }

        unsafe {
            let hook = SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(Self::keyboard_hook_proc),
                None,
                0,
            ).map_err(|e| OsError::KeyboardHookFailed {
                reason: format!("SetWindowsHookExW failed: {:?}", e),
            })?;

            KEYBOARD_HOOK.store(hook.0 as usize, Ordering::SeqCst);
        }

        self.is_registered = true;
        tracing::info!("Keyboard hooks registered");
        Ok(())
    }

    /// Unregister keyboard hooks
    pub fn unregister_hooks(&mut self) -> Result<()> {
        if !self.is_registered {
            return Ok(()); // Not registered
        }

        let hook_value = KEYBOARD_HOOK.swap(0, Ordering::SeqCst);
        if hook_value != 0 {
            unsafe {
                let hook = HHOOK(hook_value as isize);
                let _ = UnhookWindowsHookEx(hook);
                HOTKEY_CALLBACK = None;
            }
        }

        self.is_registered = false;
        tracing::info!("Keyboard hooks unregistered");
        Ok(())
    }

    /// Check if hooks are registered
    pub fn is_registered(&self) -> bool {
        self.is_registered
    }

    /// Low-level keyboard hook procedure
    ///
    /// This is called by Windows for every keyboard event system-wide.
    unsafe extern "system" fn keyboard_hook_proc(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if code >= 0 {
            let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);

            // Check for Win+D
            if kb.vkCode == VK_D.0 as u32 {
                let win_pressed = GetAsyncKeyState(VK_LWIN.0 as i32) < 0
                    || GetAsyncKeyState(VK_RWIN.0 as i32) < 0;

                if win_pressed {
                    // Intercept Win+D
                    if let Some(ref callback) = HOTKEY_CALLBACK {
                        callback(HotkeyEvent::WinD);
                    }
                    
                    // Return 1 to block the default Win+D behavior
                    return LRESULT(1);
                }
            }

            // Add more hotkey checks here as needed
            // For example, custom toggle hotkey, search hotkey, etc.
        }

        // Pass to next hook in chain
        let hook_value = KEYBOARD_HOOK.load(Ordering::SeqCst);
        let hook = if hook_value != 0 {
            Some(HHOOK(hook_value as isize))
        } else {
            None
        };
        
        CallNextHookEx(hook, code, wparam, lparam)
    }
}

impl Default for KeyboardHookManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for KeyboardHookManager {
    fn drop(&mut self) {
        let _ = self.unregister_hooks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyboard_manager_creation() {
        let manager = KeyboardHookManager::new();
        assert!(!manager.is_registered());
    }

    #[test]
    fn test_keyboard_manager_default() {
        let manager = KeyboardHookManager::default();
        assert!(!manager.is_registered());
    }
}
