//! Update module for NeuralFS
//!
//! This module provides functionality for:
//! - Model downloading with multi-source support
//! - Resume capability for interrupted downloads
//! - Checksum verification for integrity
//! - Application self-update with atomic swap & restart

pub mod model;
pub mod self_update;

#[cfg(test)]
mod tests;

pub use model::{
    ModelDownloader, ModelDownloaderConfig, ModelSource, ModelManifest, ModelInfo,
    ModelType, DownloadProgress, DownloadStatus, DownloadError,
};

pub use self_update::{
    SelfUpdater, SelfUpdaterConfig, UpdateInfo, UpdateProgress, UpdatePhase,
    UpdateStatus, UpdateChannel, UpdateError, Version, WatchdogCommand, WatchdogIpc,
    UpdateCoordinator, UpdateState, AtomicUpdateResult,
};
