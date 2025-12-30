//! Windows-specific OS integration
//!
//! This module provides Windows-specific implementations for:
//! - Desktop takeover via WorkerW
//! - Low-level keyboard hooks
//! - Taskbar control
//! - Multi-monitor support
//! - Display change monitoring

pub mod desktop;
pub mod keyboard;
pub mod taskbar;
pub mod monitor;
pub mod handle_manager;
pub mod display_listener;

pub use desktop::WindowsDesktopManager;
pub use display_listener::DisplayChangeListener;
