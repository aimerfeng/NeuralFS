//! NeuralFS - AI-driven immersive file system shell
//! 
//! This crate provides the core functionality for NeuralFS including:
//! - Semantic search with intent parsing
//! - Intelligent tag management
//! - Logic chain file associations
//! - Hybrid local/cloud inference
//! - Process supervision via watchdog
//! - OS integration (desktop takeover, hotkeys, multi-monitor)
//! - SQLite database with WAL mode for high concurrency
//! - Full-text search with multi-language tokenization

pub mod core;
pub mod db;
pub mod watchdog;
pub mod os;
pub mod vector;
pub mod search;

// Re-export commonly used items
pub use core::error::{NeuralFSError, Result};
pub use core::config::AppConfig;
pub use db::{DatabaseConfig, create_database_pool, WalCheckpointManager};
pub use os::{DesktopManager, MonitorInfo, MultiMonitorStrategy};
pub use vector::{VectorStore, VectorStoreConfig, VectorError};
pub use search::{TextIndex, TextIndexConfig, TextIndexError, MultilingualTokenizer};
