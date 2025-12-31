//! Configuration Management Module for NeuralFS
//!
//! Provides persistent configuration storage with:
//! - JSON file-based storage
//! - Configuration migration between versions
//! - Import/export functionality
//! - Thread-safe access
//!
//! **Validates: Requirements 15.7**

mod storage;
mod migration;
#[cfg(test)]
mod tests;

pub use storage::{
    ConfigStore, ConfigStoreConfig, ConfigError, ConfigResult,
    AppConfig, CloudConfig, PerformanceConfig, PrivacyConfig, UIConfig,
};
pub use migration::{
    ConfigMigration, MigrationManager, MigrationError, MigrationResult,
    ConfigVersion, VersionedConfig,
};
