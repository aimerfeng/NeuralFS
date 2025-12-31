//! Tests for Configuration Module

use super::*;
use std::path::PathBuf;
use tempfile::TempDir;

/// Create a test config store with temporary directory
async fn create_test_store() -> (ConfigStore, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.json");
    let backup_dir = temp_dir.path().join("backups");

    let settings = ConfigStoreConfig {
        config_path,
        backup_dir,
        max_backups: 3,
        create_default: true,
    };

    let store = ConfigStore::new(settings).await.unwrap();
    (store, temp_dir)
}

#[tokio::test]
async fn test_create_default_config() {
    let (store, _temp) = create_test_store().await;
    
    let config = store.get().await;
    assert_eq!(config.version, 1);
    assert!(config.monitored_directories.is_empty());
    assert!(!config.cloud.enabled);
    assert_eq!(config.ui.theme, "dark");
}

#[tokio::test]
async fn test_update_config() {
    let (store, _temp) = create_test_store().await;
    
    let updated = store.update(|config| {
        config.ui.theme = "light".to_string();
        config.cloud.enabled = true;
    }).await.unwrap();
    
    assert_eq!(updated.ui.theme, "light");
    assert!(updated.cloud.enabled);
    
    // Verify persistence
    let reloaded = store.get().await;
    assert_eq!(reloaded.ui.theme, "light");
}

#[tokio::test]
async fn test_add_monitored_directory() {
    let (store, temp) = create_test_store().await;
    
    let test_dir = temp.path().to_path_buf();
    let updated = store.add_monitored_directory(test_dir.clone()).await.unwrap();
    
    assert!(updated.monitored_directories.contains(&test_dir));
    
    // Adding same directory again should not duplicate
    let updated2 = store.add_monitored_directory(test_dir.clone()).await.unwrap();
    assert_eq!(updated2.monitored_directories.len(), 1);
}

#[tokio::test]
async fn test_remove_monitored_directory() {
    let (store, temp) = create_test_store().await;
    
    let test_dir = temp.path().to_path_buf();
    store.add_monitored_directory(test_dir.clone()).await.unwrap();
    
    let updated = store.remove_monitored_directory(&test_dir).await.unwrap();
    assert!(!updated.monitored_directories.contains(&test_dir));
}

#[tokio::test]
async fn test_set_theme() {
    let (store, _temp) = create_test_store().await;
    
    let updated = store.set_theme("light".to_string()).await.unwrap();
    assert_eq!(updated.ui.theme, "light");
}

#[tokio::test]
async fn test_set_cloud_enabled() {
    let (store, _temp) = create_test_store().await;
    
    let updated = store.set_cloud_enabled(true).await.unwrap();
    assert!(updated.cloud.enabled);
    
    let updated2 = store.set_cloud_enabled(false).await.unwrap();
    assert!(!updated2.cloud.enabled);
}

#[tokio::test]
async fn test_privacy_mode_disables_cloud() {
    let (store, _temp) = create_test_store().await;
    
    // Enable cloud first
    store.set_cloud_enabled(true).await.unwrap();
    
    // Enable privacy mode - should disable cloud
    let updated = store.set_privacy_mode(true).await.unwrap();
    assert!(updated.privacy.privacy_mode);
    assert!(!updated.cloud.enabled);
}

#[tokio::test]
async fn test_backup_creation() {
    let (store, _temp) = create_test_store().await;
    
    // Make several updates to create backups
    for i in 0..5 {
        store.update(|config| {
            config.ui.theme = format!("theme_{}", i);
        }).await.unwrap();
    }
    
    let backups = store.list_backups().await.unwrap();
    // Should have at most max_backups (3)
    assert!(backups.len() <= 3);
}

#[tokio::test]
async fn test_export_import() {
    let (store, temp) = create_test_store().await;
    
    // Modify config
    store.update(|config| {
        config.ui.theme = "light".to_string();
        config.cloud.monthly_cost_limit = 50.0;
    }).await.unwrap();
    
    // Export
    let export_path = temp.path().join("exported.json");
    store.export(&export_path).await.unwrap();
    
    // Reset and import
    store.reset().await.unwrap();
    let config = store.get().await;
    assert_eq!(config.ui.theme, "dark"); // Default
    
    let imported = store.import(&export_path).await.unwrap();
    assert_eq!(imported.ui.theme, "light");
    assert_eq!(imported.cloud.monthly_cost_limit, 50.0);
}

#[tokio::test]
async fn test_reset_config() {
    let (store, _temp) = create_test_store().await;
    
    // Modify config
    store.update(|config| {
        config.ui.theme = "light".to_string();
    }).await.unwrap();
    
    // Reset
    let reset = store.reset().await.unwrap();
    assert_eq!(reset.ui.theme, "dark");
}

// Migration tests
#[test]
fn test_migration_manager_creation() {
    let manager = MigrationManager::new();
    assert_eq!(manager.current_version(), CURRENT_VERSION);
}

#[test]
fn test_needs_migration() {
    let manager = MigrationManager::new();
    
    assert!(manager.needs_migration(0));
    assert!(!manager.needs_migration(CURRENT_VERSION));
    assert!(!manager.needs_migration(CURRENT_VERSION + 1));
}

#[test]
fn test_validate_config_valid() {
    let config = AppConfig::default();
    assert!(validate_config(&config).is_ok());
}

#[test]
fn test_validate_config_invalid_vram() {
    let mut config = AppConfig::default();
    config.performance.max_vram_mb = 100; // Too low
    
    let result = validate_config(&config);
    assert!(result.is_err());
}

#[test]
fn test_validate_config_invalid_threads() {
    let mut config = AppConfig::default();
    config.performance.indexing_threads = 0;
    
    let result = validate_config(&config);
    assert!(result.is_err());
}

#[test]
fn test_validate_config_invalid_theme() {
    let mut config = AppConfig::default();
    config.ui.theme = "invalid_theme".to_string();
    
    let result = validate_config(&config);
    assert!(result.is_err());
}

#[test]
fn test_default_excluded_patterns() {
    let config = AppConfig::default();
    
    assert!(config.privacy.excluded_patterns.contains(&"*.tmp".to_string()));
    assert!(config.privacy.excluded_patterns.contains(&"node_modules".to_string()));
    assert!(config.privacy.excluded_patterns.contains(&".git".to_string()));
}
