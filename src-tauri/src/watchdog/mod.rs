//! Watchdog Module
//!
//! This module provides process supervision functionality for NeuralFS.
//! The watchdog monitors the main application process via shared memory heartbeat
//! and can restart it if it becomes unresponsive.

pub mod heartbeat;
pub mod shared_memory;
pub mod supervisor;

#[cfg(test)]
mod tests;

pub use heartbeat::HeartbeatSender;
pub use shared_memory::{SharedMemory, HeartbeatData, HEARTBEAT_INTERVAL_MS, HEARTBEAT_TIMEOUT_MS};
pub use supervisor::{Watchdog, WatchdogConfig, WatchdogError, WatchdogState, restore_windows_explorer, start_main_process};
