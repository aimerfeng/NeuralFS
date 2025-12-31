//! Configuration Migration Module
//!
//! Handles configuration schema migrations between versions:
//! - Version detection
//! - Automatic migration on load
//! - Rollback support
//!
//! **Validates: Requirements 15.7**

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use thiserror::Error;

use super::storage::{AppConfig, ConfigResult, ConfigError};

/// Current configuration version
pub const CURRENT_VERSION: u32 = 1;

/// Migration error types
#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("Unknown config version: {0}")]
    UnknownVersion(u32),

    #[error("Migration failed from v{from} to v{to}: {reason}")]
    MigrationFailed {
        from: u32,
        to: u32,
        reason: String,
    },

    #[error("Rollback failed: {0}")]
    RollbackFailed(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Migration result type
pub type MigrationResult<T> = Result<T, MigrationError>;

/// Configuration version info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigVersion {
    /// Version number
    pub version: u32,
    /// Version description
    pub description: String,
    /// Release date
    pub release_date: String,
}

/// Versioned configuration wrapper for migration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedConfig {
    /// Configuration version
    #[serde(default = "default_version")]
    pub version: u32,
    /// Raw configuration data
    #[serde(flatten)]
    pub data: Value,
}

fn default_version() -> u32 {
    1
}

/// Configuration migration trait
pub trait ConfigMigration: Send + Sync {
    /// Source version
    fn from_version(&self) -> u32;
    
    /// Target version
    fn to_version(&self) -> u32;
    
    /// Perform migration
    fn migrate(&self, config: Value) -> MigrationResult<Value>;
    
    /// Rollback migration (optional)
    fn rollback(&self, config: Value) -> MigrationResult<Value> {
        Err(MigrationError::RollbackFailed(
            "Rollback not supported for this migration".to_string()
        ))
    }
}

/// Migration manager handles version upgrades
pub struct MigrationManager {
    migrations: Vec<Box<dyn ConfigMigration>>,
}

impl MigrationManager {
    /// Create a new migration manager with all registered migrations
    pub fn new() -> Self {
        let mut manager = Self {
            migrations: Vec::new(),
        };
        
        // Register all migrations here
        // Example: manager.register(Box::new(MigrationV1ToV2));
        
        manager
    }

    /// Register a migration
    pub fn register(&mut self, migration: Box<dyn ConfigMigration>) {
        self.migrations.push(migration);
    }

    /// Get current version
    pub fn current_version(&self) -> u32 {
        CURRENT_VERSION
    }

    /// Check if migration is needed
    pub fn needs_migration(&self, config_version: u32) -> bool {
        config_version < CURRENT_VERSION
    }

    /// Migrate configuration to current version
    pub fn migrate(&self, mut config: Value) -> MigrationResult<Value> {
        let mut current_version = config
            .get("version")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(1);

        while current_version < CURRENT_VERSION {
            let migration = self.find_migration(current_version)?;
            config = migration.migrate(config)?;
            current_version = migration.to_version();
        }

        // Update version in config
        if let Some(obj) = config.as_object_mut() {
            obj.insert("version".to_string(), Value::Number(CURRENT_VERSION.into()));
        }

        Ok(config)
    }

    /// Find migration for given version
    fn find_migration(&self, from_version: u32) -> MigrationResult<&dyn ConfigMigration> {
        self.migrations
            .iter()
            .find(|m| m.from_version() == from_version)
            .map(|m| m.as_ref())
            .ok_or(MigrationError::UnknownVersion(from_version))
    }

    /// Load and migrate configuration file
    pub async fn load_and_migrate(&self, path: &Path) -> MigrationResult<AppConfig> {
        let content = tokio::fs::read_to_string(path).await?;
        let config: Value = serde_json::from_str(&content)?;
        
        let version = config
            .get("version")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(1);

        let migrated = if self.needs_migration(version) {
            self.migrate(config)?
        } else {
            config
        };

        let app_config: AppConfig = serde_json::from_value(migrated)?;
        Ok(app_config)
    }

    /// Get migration history
    pub fn get_migration_history(&self) -> Vec<ConfigVersion> {
        vec![
            ConfigVersion {
                version: 1,
                description: "Initial configuration schema".to_string(),
                release_date: "2024-01-01".to_string(),
            },
            // Add more versions as migrations are added
        ]
    }
}

impl Default for MigrationManager {
    fn default() -> Self {
        Self::new()
    }
}

// Example migration implementation (for future use)
// When adding new config fields, create a migration like this:

/*
/// Migration from v1 to v2
struct MigrationV1ToV2;

impl ConfigMigration for MigrationV1ToV2 {
    fn from_version(&self) -> u32 { 1 }
    fn to_version(&self) -> u32 { 2 }
    
    fn migrate(&self, mut config: Value) -> MigrationResult<Value> {
        if let Some(obj) = config.as_object_mut() {
            // Add new field with default value
            if !obj.contains_key("new_field") {
                obj.insert("new_field".to_string(), Value::String("default".to_string()));
            }
            
            // Rename field
            if let Some(old_value) = obj.remove("old_field_name") {
                obj.insert("new_field_name".to_string(), old_value);
            }
            
            // Transform field value
            if let Some(value) = obj.get_mut("some_field") {
                // Transform logic here
            }
        }
        Ok(config)
    }
    
    fn rollback(&self, mut config: Value) -> MigrationResult<Value> {
        if let Some(obj) = config.as_object_mut() {
            // Reverse the migration
            obj.remove("new_field");
            
            if let Some(new_value) = obj.remove("new_field_name") {
                obj.insert("old_field_name".to_string(), new_value);
            }
        }
        Ok(config)
    }
}
*/

/// Validate configuration structure
pub fn validate_config(config: &AppConfig) -> ConfigResult<()> {
    // Validate monitored directories exist
    for dir in &config.monitored_directories {
        if !dir.exists() {
            // Log warning but don't fail - directory might be on removable media
            tracing::warn!("Monitored directory does not exist: {:?}", dir);
        }
    }

    // Validate cloud config
    if config.cloud.enabled {
        if config.cloud.api_key.is_none() && config.cloud.endpoint.is_none() {
            return Err(ConfigError::Invalid(
                "Cloud enabled but no API key or endpoint configured".to_string()
            ));
        }
    }

    // Validate performance config
    if config.performance.max_vram_mb < 512 {
        return Err(ConfigError::Invalid(
            "max_vram_mb must be at least 512 MB".to_string()
        ));
    }

    if config.performance.indexing_threads == 0 {
        return Err(ConfigError::Invalid(
            "indexing_threads must be at least 1".to_string()
        ));
    }

    // Validate UI config
    let valid_themes = ["light", "dark", "system"];
    if !valid_themes.contains(&config.ui.theme.as_str()) {
        return Err(ConfigError::Invalid(
            format!("Invalid theme: {}. Must be one of: {:?}", config.ui.theme, valid_themes)
        ));
    }

    Ok(())
}
