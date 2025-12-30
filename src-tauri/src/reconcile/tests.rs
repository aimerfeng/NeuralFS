//! Tests for the reconciliation module

use super::*;
use tempfile::TempDir;
use std::fs::File;
use std::io::Write;

/// Helper to create a test database
async fn create_test_db() -> (SqlitePool, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    
    let config = crate::db::DatabaseConfig::with_path(db_path).with_wal(true);
    let pool = crate::db::create_database_pool(&config).await.unwrap();
    
    // Run migrations
    let migration_manager = crate::db::migration::MigrationManager::new(pool.clone());
    migration_manager.run_migrations().await.unwrap();
    
    (pool, temp_dir)
}

/// Helper to create a test file
fn create_test_file(dir: &Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    let mut file = File::create(&path).unwrap();
    file.write_all(content.as_bytes()).unwrap();
    path
}

#[tokio::test]
async fn test_file_id_from_path() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = create_test_file(temp_dir.path(), "test.txt", "hello world");
    
    let file_id = FileId::from_path(&file_path).unwrap();
    
    // FileID should be consistent for the same file
    let file_id2 = FileId::from_path(&file_path).unwrap();
    assert_eq!(file_id, file_id2);
}

#[tokio::test]
async fn test_file_id_string_roundtrip() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = create_test_file(temp_dir.path(), "test.txt", "hello world");
    
    let file_id = FileId::from_path(&file_path).unwrap();
    let string_repr = file_id.to_string_repr();
    let parsed = FileId::from_string_repr(&string_repr).unwrap();
    
    assert_eq!(file_id, parsed);
}

#[tokio::test]
async fn test_reconciliation_service_creation() {
    let (pool, _temp_dir) = create_test_db().await;
    
    let service = ReconciliationService::new(pool);
    let stats = service.get_stats().await.unwrap();
    
    assert_eq!(stats.total_files, 0);
    assert_eq!(stats.files_with_file_id, 0);
}

#[tokio::test]
async fn test_reconcile_detects_new_files() {
    let (pool, db_temp_dir) = create_test_db().await;
    
    // Create a directory with test files
    let files_dir = TempDir::new().unwrap();
    create_test_file(files_dir.path(), "file1.txt", "content 1");
    create_test_file(files_dir.path(), "file2.txt", "content 2");
    
    let service = ReconciliationService::new(pool);
    let result = service
        .reconcile_on_startup(&[files_dir.path().to_path_buf()])
        .await
        .unwrap();
    
    // Should detect 2 new files
    assert_eq!(result.added.len(), 2);
    assert!(result.deleted.is_empty());
    assert!(result.modified.is_empty());
    assert!(result.renamed.is_empty());
    
    // Cleanup
    drop(db_temp_dir);
}

#[tokio::test]
async fn test_reconcile_detects_deleted_files() {
    let (pool, db_temp_dir) = create_test_db().await;
    
    // Create a directory with test files
    let files_dir = TempDir::new().unwrap();
    let file_path = create_test_file(files_dir.path(), "file1.txt", "content 1");
    
    let service = ReconciliationService::new(pool.clone());
    
    // First reconciliation - adds the file
    let result = service
        .reconcile_on_startup(&[files_dir.path().to_path_buf()])
        .await
        .unwrap();
    assert_eq!(result.added.len(), 1);
    
    // Delete the file
    std::fs::remove_file(&file_path).unwrap();
    
    // Second reconciliation - should detect deletion
    let result = service
        .reconcile_on_startup(&[files_dir.path().to_path_buf()])
        .await
        .unwrap();
    
    assert!(result.added.is_empty());
    assert_eq!(result.deleted.len(), 1);
    
    // Cleanup
    drop(db_temp_dir);
}

#[tokio::test]
async fn test_reconcile_detects_modified_files() {
    let (pool, db_temp_dir) = create_test_db().await;
    
    // Create a directory with test files
    let files_dir = TempDir::new().unwrap();
    let file_path = create_test_file(files_dir.path(), "file1.txt", "content 1");
    
    let service = ReconciliationService::new(pool.clone());
    
    // First reconciliation - adds the file
    let result = service
        .reconcile_on_startup(&[files_dir.path().to_path_buf()])
        .await
        .unwrap();
    assert_eq!(result.added.len(), 1);
    
    // Wait a bit and modify the file
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let mut file = File::create(&file_path).unwrap();
    file.write_all(b"modified content that is longer").unwrap();
    
    // Second reconciliation - should detect modification
    let result = service
        .reconcile_on_startup(&[files_dir.path().to_path_buf()])
        .await
        .unwrap();
    
    assert!(result.added.is_empty());
    assert!(result.deleted.is_empty());
    assert_eq!(result.modified.len(), 1);
    
    // Cleanup
    drop(db_temp_dir);
}

#[tokio::test]
async fn test_reconcile_result_has_changes() {
    let mut result = ReconcileResult::default();
    assert!(!result.has_changes());
    assert_eq!(result.total_changes(), 0);
    
    result.added.push(PathBuf::from("/test/file.txt"));
    assert!(result.has_changes());
    assert_eq!(result.total_changes(), 1);
    
    result.deleted.push(PathBuf::from("/test/deleted.txt"));
    assert_eq!(result.total_changes(), 2);
}

#[tokio::test]
async fn test_reconcile_config_default() {
    let config = ReconcileConfig::default();
    
    assert_eq!(config.max_parallel_scans, 4);
    assert_eq!(config.batch_size, 1000);
    assert!(config.fast_mode);
    assert!(!config.verify_hash);
}

#[tokio::test]
async fn test_file_id_cache() {
    let (pool, _db_temp_dir) = create_test_db().await;
    let temp_dir = TempDir::new().unwrap();
    let file_path = create_test_file(temp_dir.path(), "test.txt", "hello");
    
    let service = ReconciliationService::new(pool);
    
    // First call should fetch from filesystem
    let file_id1 = service.get_file_id(&file_path).await.unwrap();
    
    // Second call should use cache
    let file_id2 = service.get_file_id(&file_path).await.unwrap();
    
    assert_eq!(file_id1, file_id2);
    
    // Clear cache
    service.clear_cache().await;
    
    // Should still work after clearing cache
    let file_id3 = service.get_file_id(&file_path).await.unwrap();
    assert_eq!(file_id1, file_id3);
}


#[tokio::test]
async fn test_reconcile_detects_renamed_files() {
    let (pool, db_temp_dir) = create_test_db().await;
    
    // Create a directory with test files
    let files_dir = TempDir::new().unwrap();
    let original_path = create_test_file(files_dir.path(), "original.txt", "content");
    
    let service = ReconciliationService::new(pool.clone());
    
    // First reconciliation - adds the file
    let result = service
        .reconcile_on_startup(&[files_dir.path().to_path_buf()])
        .await
        .unwrap();
    assert_eq!(result.added.len(), 1);
    
    // Get the FileID before rename
    let file_id_before = FileId::from_path(&original_path).unwrap();
    
    // Rename the file
    let new_path = files_dir.path().join("renamed.txt");
    std::fs::rename(&original_path, &new_path).unwrap();
    
    // Get the FileID after rename - should be the same
    let file_id_after = FileId::from_path(&new_path).unwrap();
    assert_eq!(file_id_before, file_id_after, "FileID should be preserved across rename");
    
    // Second reconciliation - should detect rename
    let result = service
        .reconcile_on_startup(&[files_dir.path().to_path_buf()])
        .await
        .unwrap();
    
    // Should detect as rename, not as delete + add
    assert_eq!(result.renamed.len(), 1, "Should detect 1 rename");
    assert!(result.added.is_empty(), "Should not detect as new file");
    assert!(result.deleted.is_empty(), "Should not detect as deleted");
    
    // Verify rename details
    let rename = &result.renamed[0];
    assert_eq!(rename.old_path, original_path);
    assert_eq!(rename.new_path, new_path);
    assert_eq!(rename.file_id, file_id_before);
    
    // Cleanup
    drop(db_temp_dir);
}

#[tokio::test]
async fn test_file_id_preserved_across_rename() {
    let temp_dir = TempDir::new().unwrap();
    let original_path = create_test_file(temp_dir.path(), "original.txt", "test content");
    
    // Get FileID before rename
    let file_id_before = FileId::from_path(&original_path).unwrap();
    
    // Rename the file
    let new_path = temp_dir.path().join("renamed.txt");
    std::fs::rename(&original_path, &new_path).unwrap();
    
    // Get FileID after rename
    let file_id_after = FileId::from_path(&new_path).unwrap();
    
    // FileID should be the same
    assert_eq!(file_id_before, file_id_after);
}

#[tokio::test]
async fn test_different_files_have_different_ids() {
    let temp_dir = TempDir::new().unwrap();
    let file1 = create_test_file(temp_dir.path(), "file1.txt", "content 1");
    let file2 = create_test_file(temp_dir.path(), "file2.txt", "content 2");
    
    let id1 = FileId::from_path(&file1).unwrap();
    let id2 = FileId::from_path(&file2).unwrap();
    
    // Different files should have different IDs
    assert_ne!(id1, id2);
}

#[tokio::test]
async fn test_update_file_id_in_database() {
    let (pool, _db_temp_dir) = create_test_db().await;
    let temp_dir = TempDir::new().unwrap();
    let file_path = create_test_file(temp_dir.path(), "test.txt", "hello");
    
    let service = ReconciliationService::new(pool.clone());
    
    // First, add the file via reconciliation
    let result = service
        .reconcile_on_startup(&[temp_dir.path().to_path_buf()])
        .await
        .unwrap();
    assert_eq!(result.added.len(), 1);
    
    // Update the FileID
    let file_id = service.update_file_id(&file_path).await.unwrap();
    
    // Verify it was stored
    let stats = service.get_stats().await.unwrap();
    assert_eq!(stats.files_with_file_id, 1);
    
    // Verify the FileID matches
    let retrieved_id = service.get_file_id(&file_path).await.unwrap();
    assert_eq!(file_id, retrieved_id);
}


// Property-based tests using proptest
#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    /// Generate a valid filename (alphanumeric with extension)
    fn valid_filename_strategy() -> impl Strategy<Value = String> {
        "[a-zA-Z][a-zA-Z0-9_]{0,20}\\.(txt|md|rs|json|xml)"
            .prop_filter("filename must be valid", |s| !s.is_empty() && s.len() < 50)
    }

    /// Generate file content
    fn file_content_strategy() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 \n]{1,100}"
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Feature: neural-fs-core, Property 21: File ID Tracking Across Renames**
        /// *For any* file that is renamed in the filesystem, the ReconciliationService 
        /// SHALL detect the rename and preserve all associated tags and relations.
        /// **Validates: Requirements 8.4, Reconciliation Strategy**
        #[test]
        fn prop_file_id_preserved_across_rename(
            original_name in valid_filename_strategy(),
            new_name in valid_filename_strategy(),
            content in file_content_strategy()
        ) {
            // Skip if names are the same
            if original_name == new_name {
                return Ok(());
            }

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let temp_dir = TempDir::new().unwrap();
                
                // Create original file
                let original_path = temp_dir.path().join(&original_name);
                let mut file = File::create(&original_path).unwrap();
                file.write_all(content.as_bytes()).unwrap();
                drop(file);
                
                // Get FileID before rename
                let file_id_before = FileId::from_path(&original_path).unwrap();
                
                // Rename the file
                let new_path = temp_dir.path().join(&new_name);
                std::fs::rename(&original_path, &new_path).unwrap();
                
                // Get FileID after rename
                let file_id_after = FileId::from_path(&new_path).unwrap();
                
                // Property: FileID MUST be preserved across rename
                prop_assert_eq!(
                    file_id_before, 
                    file_id_after,
                    "FileID must be preserved when file is renamed from {} to {}",
                    original_name,
                    new_name
                );
                
                Ok(())
            })?;
            
            Ok(())
        }

        /// **Feature: neural-fs-core, Property 21: File ID Tracking Across Renames (Reconciliation)**
        /// *For any* file that is renamed, the ReconciliationService SHALL detect it as a rename
        /// rather than a delete + add operation.
        /// **Validates: Requirements 8.4, Reconciliation Strategy**
        #[test]
        fn prop_reconciliation_detects_rename(
            original_name in valid_filename_strategy(),
            new_name in valid_filename_strategy(),
            content in file_content_strategy()
        ) {
            // Skip if names are the same
            if original_name == new_name {
                return Ok(());
            }

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                // Create test database
                let temp_db_dir = TempDir::new().unwrap();
                let db_path = temp_db_dir.path().join("test.db");
                let config = crate::db::DatabaseConfig::with_path(db_path).with_wal(true);
                let pool = crate::db::create_database_pool(&config).await.unwrap();
                
                // Run migrations
                let migration_manager = crate::db::migration::MigrationManager::new(pool.clone());
                migration_manager.run_migrations().await.unwrap();
                
                // Create files directory
                let files_dir = TempDir::new().unwrap();
                
                // Create original file
                let original_path = files_dir.path().join(&original_name);
                let mut file = File::create(&original_path).unwrap();
                file.write_all(content.as_bytes()).unwrap();
                drop(file);
                
                let service = ReconciliationService::new(pool.clone());
                
                // First reconciliation - adds the file
                let result = service
                    .reconcile_on_startup(&[files_dir.path().to_path_buf()])
                    .await
                    .unwrap();
                prop_assert_eq!(result.added.len(), 1, "Should add 1 file initially");
                
                // Rename the file
                let new_path = files_dir.path().join(&new_name);
                std::fs::rename(&original_path, &new_path).unwrap();
                
                // Second reconciliation - should detect rename
                let result = service
                    .reconcile_on_startup(&[files_dir.path().to_path_buf()])
                    .await
                    .unwrap();
                
                // Property: Rename MUST be detected as rename, not delete + add
                prop_assert_eq!(
                    result.renamed.len(), 
                    1, 
                    "Should detect exactly 1 rename for {} -> {}",
                    original_name,
                    new_name
                );
                prop_assert!(
                    result.added.is_empty(),
                    "Should NOT detect renamed file as new"
                );
                prop_assert!(
                    result.deleted.is_empty(),
                    "Should NOT detect renamed file as deleted"
                );
                
                // Verify rename details
                let rename = &result.renamed[0];
                prop_assert_eq!(rename.old_path, original_path);
                prop_assert_eq!(rename.new_path, new_path);
                
                Ok(())
            })?;
            
            Ok(())
        }

        /// **Feature: neural-fs-core, Property 21: FileID String Serialization Round-Trip**
        /// *For any* FileID, serializing to string and parsing back should produce the same FileID.
        /// **Validates: Reconciliation Strategy**
        #[test]
        fn prop_file_id_string_roundtrip(
            filename in valid_filename_strategy(),
            content in file_content_strategy()
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let temp_dir = TempDir::new().unwrap();
                
                // Create file
                let file_path = temp_dir.path().join(&filename);
                let mut file = File::create(&file_path).unwrap();
                file.write_all(content.as_bytes()).unwrap();
                drop(file);
                
                // Get FileID
                let file_id = FileId::from_path(&file_path).unwrap();
                
                // Serialize to string
                let string_repr = file_id.to_string_repr();
                
                // Parse back
                let parsed = FileId::from_string_repr(&string_repr);
                
                // Property: Round-trip must preserve FileID
                prop_assert!(parsed.is_some(), "FileID string should be parseable");
                prop_assert_eq!(file_id, parsed.unwrap(), "FileID round-trip must be identity");
                
                Ok(())
            })?;
            
            Ok(())
        }
    }
}
