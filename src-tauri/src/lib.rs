//! NeuralFS - AI-driven immersive file system shell
//! 
//! This crate provides the core functionality for NeuralFS including:
//! - Semantic search with intent parsing
//! - Intelligent tag management
//! - Logic chain file associations
//! - Hybrid local/cloud inference
//! - Process supervision via watchdog
//! - OS integration (desktop takeover, hotkeys, multi-monitor)

pub mod core;
pub mod watchdog;
pub mod os;

// Re-export commonly used items
pub use core::error::{NeuralFSError, Result};
pub use core::config::AppConfig;
pub use os::{DesktopManager, MonitorInfo, MultiMonitorStrategy};
