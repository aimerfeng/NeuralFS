//! NeuralFS Core Module
//! 
//! This module contains the core functionality for NeuralFS including:
//! - Configuration management
//! - Error types and handling
//! - Core data types
//! - Utility functions
//! - Runtime dependency management

pub mod config;
pub mod error;
pub mod runtime;
pub mod types;
pub mod utils;

// Re-export commonly used items
pub use config::*;
pub use error::{NeuralFSError, Result};
pub use runtime::{RuntimeDependencies, RuntimeStatus};
