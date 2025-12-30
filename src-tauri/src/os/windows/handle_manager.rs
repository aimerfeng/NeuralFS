//! Window Handle Lifecycle Manager
//!
//! Manages the mapping between Tauri window handles and Win32 HWNDs,
//! tracking changes when Webviews are rebuilt.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::core::error::{OsError, Result};

/// Window Handle Manager
///
/// Tracks window handles and their lifecycle, ensuring that operations
/// like SetParent always use the most current HWND.
pub struct WindowHandleManager {
    /// Current main window handle
    current_hwnd: AtomicUsize,
    /// History of window handles (for debugging)
    handle_history: Vec<HandleChange>,
    /// Named window handles (for multi-window support)
    named_handles: HashMap<String, usize>,
}

/// Record of a handle change
#[derive(Debug, Clone)]
pub struct HandleChange {
    /// Previous handle value
    pub old_hwnd: usize,
    /// New handle value
    pub new_hwnd: usize,
    /// Timestamp of change
    pub timestamp: std::time::Instant,
    /// Reason for change
    pub reason: HandleChangeReason,
}

/// Reason for handle change
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleChangeReason {
    /// Initial handle assignment
    Initial,
    /// Webview was rebuilt
    WebviewRebuild,
    /// Window was recreated
    WindowRecreate,
    /// Manual update
    ManualUpdate,
}

impl WindowHandleManager {
    /// Create a new handle manager
    pub fn new() -> Self {
        Self {
            current_hwnd: AtomicUsize::new(0),
            handle_history: Vec::new(),
            named_handles: HashMap::new(),
        }
    }

    /// Set the current window handle
    pub fn set_current_hwnd(&mut self, hwnd: usize) {
        self.set_current_hwnd_with_reason(hwnd, HandleChangeReason::ManualUpdate);
    }

    /// Set the current window handle with a specific reason
    pub fn set_current_hwnd_with_reason(&mut self, hwnd: usize, reason: HandleChangeReason) {
        let old_hwnd = self.current_hwnd.swap(hwnd, Ordering::SeqCst);
        
        if old_hwnd != hwnd {
            self.handle_history.push(HandleChange {
                old_hwnd,
                new_hwnd: hwnd,
                timestamp: std::time::Instant::now(),
                reason,
            });

            tracing::debug!(
                "Window handle changed: {:?} -> {:?} (reason: {:?})",
                old_hwnd,
                hwnd,
                reason
            );
        }
    }

    /// Get the current window handle
    pub fn get_current_hwnd(&self) -> Option<usize> {
        let hwnd = self.current_hwnd.load(Ordering::SeqCst);
        if hwnd == 0 {
            None
        } else {
            Some(hwnd)
        }
    }

    /// Check if a handle is valid (non-zero)
    pub fn is_valid(&self) -> bool {
        self.current_hwnd.load(Ordering::SeqCst) != 0
    }

    /// Register a named window handle
    pub fn register_named_handle(&mut self, name: &str, hwnd: usize) {
        self.named_handles.insert(name.to_string(), hwnd);
        tracing::debug!("Registered named handle '{}': {:?}", name, hwnd);
    }

    /// Get a named window handle
    pub fn get_named_handle(&self, name: &str) -> Option<usize> {
        self.named_handles.get(name).copied()
    }

    /// Remove a named window handle
    pub fn remove_named_handle(&mut self, name: &str) -> Option<usize> {
        self.named_handles.remove(name)
    }

    /// Get handle change history
    pub fn get_history(&self) -> &[HandleChange] {
        &self.handle_history
    }

    /// Clear handle history (keep only last N entries)
    pub fn trim_history(&mut self, keep_last: usize) {
        if self.handle_history.len() > keep_last {
            let drain_count = self.handle_history.len() - keep_last;
            self.handle_history.drain(0..drain_count);
        }
    }

    /// Notify that a Webview rebuild occurred
    pub fn notify_webview_rebuild(&mut self, new_hwnd: usize) {
        self.set_current_hwnd_with_reason(new_hwnd, HandleChangeReason::WebviewRebuild);
    }

    /// Validate that the current handle matches expected
    pub fn validate_handle(&self, expected: usize) -> Result<()> {
        let current = self.current_hwnd.load(Ordering::SeqCst);
        if current != expected {
            return Err(OsError::InvalidWindowHandle {
                reason: format!(
                    "Handle mismatch: expected {:?}, got {:?}",
                    expected, current
                ),
            }.into());
        }
        Ok(())
    }
}

impl Default for WindowHandleManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_manager_creation() {
        let manager = WindowHandleManager::new();
        assert!(!manager.is_valid());
        assert!(manager.get_current_hwnd().is_none());
    }

    #[test]
    fn test_handle_manager_set_get() {
        let mut manager = WindowHandleManager::new();
        manager.set_current_hwnd(12345);
        assert!(manager.is_valid());
        assert_eq!(manager.get_current_hwnd(), Some(12345));
    }

    #[test]
    fn test_handle_manager_history() {
        let mut manager = WindowHandleManager::new();
        manager.set_current_hwnd(100);
        manager.set_current_hwnd(200);
        manager.set_current_hwnd(300);

        let history = manager.get_history();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].new_hwnd, 100);
        assert_eq!(history[1].new_hwnd, 200);
        assert_eq!(history[2].new_hwnd, 300);
    }

    #[test]
    fn test_handle_manager_named_handles() {
        let mut manager = WindowHandleManager::new();
        manager.register_named_handle("main", 100);
        manager.register_named_handle("settings", 200);

        assert_eq!(manager.get_named_handle("main"), Some(100));
        assert_eq!(manager.get_named_handle("settings"), Some(200));
        assert_eq!(manager.get_named_handle("unknown"), None);
    }

    #[test]
    fn test_handle_manager_trim_history() {
        let mut manager = WindowHandleManager::new();
        for i in 1..=10 {
            manager.set_current_hwnd(i * 100);
        }

        assert_eq!(manager.get_history().len(), 10);
        manager.trim_history(5);
        assert_eq!(manager.get_history().len(), 5);
    }

    #[test]
    fn test_handle_manager_validate() {
        let mut manager = WindowHandleManager::new();
        manager.set_current_hwnd(12345);

        assert!(manager.validate_handle(12345).is_ok());
        assert!(manager.validate_handle(99999).is_err());
    }

    #[test]
    fn test_webview_rebuild_notification() {
        let mut manager = WindowHandleManager::new();
        manager.set_current_hwnd(100);
        manager.notify_webview_rebuild(200);

        assert_eq!(manager.get_current_hwnd(), Some(200));
        
        let history = manager.get_history();
        assert_eq!(history.last().unwrap().reason, HandleChangeReason::WebviewRebuild);
    }
}
