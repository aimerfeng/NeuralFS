//! Tests for the Tag Management System
//!
//! Includes property-based tests for:
//! - Property 8: Tag Assignment Completeness
//! - Property 9: Tag Hierarchy Depth Bound
//! - Property 24: Sensitive Tag Confirmation Requirement

use super::*;
use crate::core::types::TagType;
use crate::db::{create_database_pool, DatabaseConfig};
use crate::db::migration::MigrationManager;
use proptest::prelude::*;
use sqlx::SqlitePool;
use tempfile::TempDir;
use uuid::Uuid;

/// Helper to create a test database
async fn setup_test_db() -> (SqlitePool, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    
    let config = DatabaseConfig::with_path(db_path).with_wal(false);
    let pool = create_database_pool(&config).await.unwrap();
    
    // Run migrations
    let migration_manager = MigrationManager::new(pool.clone());
    migration_manager.run_migrations().await.unwrap();
    
    (pool, temp_dir)
}

// ============================================================================
// Unit Tests
// ============================================================================

#[tokio::test]
async fn test_create_tag() {
    let (pool, _temp_dir) = setup_test_db().await;
    let manager = TagManager::new(pool, TagManagerConfig::default()).await.unwrap();
    
    let tag = manager.create_tag(
        "TestTag".to_string(),
        TagType::Custom,
        None,
        Some("#FF0000".to_string()),
        Some("🏷️".to_string()),
    ).await.unwrap();
    
    assert_eq!(tag.name, "TestTag");
    assert_eq!(tag.color, "#FF0000");
    assert_eq!(tag.icon, Some("🏷️".to_string()));
    assert!(!tag.is_system);
}

#[tokio::test]
async fn test_get_tag_by_name() {
    let (pool, _temp_dir) = setup_test_db().await;
    let manager = TagManager::new(pool, TagManagerConfig::default()).await.unwrap();
    
    let created = manager.create_tag(
        "FindMe".to_string(),
        TagType::Category,
        None,
        None,
        None,
    ).await.unwrap();
    
    let found = manager.get_tag_by_name("FindMe").await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, created.id);
    
    let not_found = manager.get_tag_by_name("NotExists").await.unwrap();
    assert!(not_found.is_none());
}

#[tokio::test]
async fn test_tag_hierarchy() {
    let (pool, _temp_dir) = setup_test_db().await;
    let manager = TagManager::new(pool, TagManagerConfig::default()).await.unwrap();
    
    // Create parent tag
    let parent = manager.create_tag(
        "Parent".to_string(),
        TagType::Category,
        None,
        None,
        None,
    ).await.unwrap();
    
    // Create child tag
    let child = manager.create_tag(
        "Child".to_string(),
        TagType::Category,
        Some(parent.id),
        None,
        None,
    ).await.unwrap();
    
    // Verify hierarchy
    let path = manager.hierarchy().get_path(child.id).await.unwrap();
    assert_eq!(path.tags.len(), 2);
    assert_eq!(path.tags[0].id, parent.id);
    assert_eq!(path.tags[1].id, child.id);
}

#[tokio::test]
async fn test_hierarchy_depth_limit() {
    let (pool, _temp_dir) = setup_test_db().await;
    let manager = TagManager::new(pool, TagManagerConfig::default()).await.unwrap();
    
    // Create level 0
    let level0 = manager.create_tag("Level0".to_string(), TagType::Category, None, None, None).await.unwrap();
    
    // Create level 1
    let level1 = manager.create_tag("Level1".to_string(), TagType::Category, Some(level0.id), None, None).await.unwrap();
    
    // Create level 2
    let level2 = manager.create_tag("Level2".to_string(), TagType::Category, Some(level1.id), None, None).await.unwrap();
    
    // Try to create level 3 - should fail
    let result = manager.create_tag("Level3".to_string(), TagType::Category, Some(level2.id), None, None).await;
    assert!(matches!(result, Err(TagError::HierarchyDepthExceeded { .. })));
}

#[tokio::test]
async fn test_add_tag_to_file() {
    let (pool, _temp_dir) = setup_test_db().await;
    let manager = TagManager::new(pool.clone(), TagManagerConfig::default()).await.unwrap();
    
    // Create a tag
    let tag = manager.create_tag("FileTag".to_string(), TagType::Custom, None, None, None).await.unwrap();
    
    // Create a fake file entry
    let file_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO files (id, path, filename, extension, file_type, size_bytes, content_hash, created_at, modified_at, indexed_at, index_status, privacy_level, is_excluded)
        VALUES (?, '/test/file.txt', 'file.txt', 'txt', 'Text', 100, 'hash123', datetime('now'), datetime('now'), datetime('now'), 'Indexed', 'Normal', 0)
        "#
    )
    .bind(file_id.to_string())
    .execute(&pool)
    .await
    .unwrap();
    
    // Add tag to file
    let relation = manager.add_tag_to_file(file_id, tag.id, TagSource::Manual, None).await.unwrap();
    
    assert_eq!(relation.file_id, file_id);
    assert_eq!(relation.tag_id, tag.id);
    assert!(relation.is_confirmed); // Manual tags are auto-confirmed
}

#[tokio::test]
async fn test_auto_tag_file() {
    let (pool, _temp_dir) = setup_test_db().await;
    let manager = TagManager::new(pool.clone(), TagManagerConfig::default()).await.unwrap();
    
    // Create a fake file entry
    let file_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO files (id, path, filename, extension, file_type, size_bytes, content_hash, created_at, modified_at, indexed_at, index_status, privacy_level, is_excluded)
        VALUES (?, '/test/document.pdf', 'document.pdf', 'pdf', 'Document', 1000, 'hash456', datetime('now'), datetime('now'), datetime('now'), 'Indexed', 'Normal', 0)
        "#
    )
    .bind(file_id.to_string())
    .execute(&pool)
    .await
    .unwrap();
    
    // Auto-tag the file
    let result = manager.auto_tag_file(
        file_id,
        "document.pdf",
        "pdf",
        Some("This is a work project report about the budget"),
    ).await.unwrap();
    
    // Should have at least the file type tag
    assert!(!result.assigned_tags.is_empty());
    
    // Verify file type tag was assigned
    let file_tags = manager.get_file_tags(file_id).await.unwrap();
    assert!(!file_tags.is_empty());
}

#[tokio::test]
async fn test_sensitive_tag_detection() {
    let detector = SensitiveTagDetector::new();
    
    // Non-sensitive tags
    assert_eq!(detector.check_sensitivity("Documents"), SensitivityLevel::None);
    assert_eq!(detector.check_sensitivity("Work"), SensitivityLevel::None);
    
    // Sensitive tags
    assert_eq!(detector.check_sensitivity("Personal Documents"), SensitivityLevel::High);
    assert_eq!(detector.check_sensitivity("Bank Statements"), SensitivityLevel::High);
    assert_eq!(detector.check_sensitivity("Medical Records"), SensitivityLevel::High);
}

#[tokio::test]
async fn test_tag_correction_confirm() {
    let (pool, _temp_dir) = setup_test_db().await;
    let manager = TagManager::new(pool.clone(), TagManagerConfig::default()).await.unwrap();
    let correction_service = TagCorrectionService::new(pool.clone());
    
    // Create tag and file
    let tag = manager.create_tag("AITag".to_string(), TagType::AutoGenerated, None, None, None).await.unwrap();
    let file_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO files (id, path, filename, extension, file_type, size_bytes, content_hash, created_at, modified_at, indexed_at, index_status, privacy_level, is_excluded)
        VALUES (?, '/test/ai_file.txt', 'ai_file.txt', 'txt', 'Text', 100, 'hash789', datetime('now'), datetime('now'), datetime('now'), 'Indexed', 'Normal', 0)
        "#
    )
    .bind(file_id.to_string())
    .execute(&pool)
    .await
    .unwrap();
    
    // Add AI-generated tag (not confirmed)
    manager.add_tag_to_file(file_id, tag.id, TagSource::AIGenerated, Some(0.8)).await.unwrap();
    
    // Confirm the tag
    let result = correction_service.execute(
        TagCommand::ConfirmTag { file_id, tag_id: tag.id },
        &manager,
    ).await.unwrap();
    
    assert!(result.success);
    
    // Verify confirmation
    let relation = manager.get_file_tag_relation(file_id, tag.id).await.unwrap().unwrap();
    assert!(relation.is_confirmed);
    assert!(!relation.is_rejected);
}

#[tokio::test]
async fn test_tag_correction_reject() {
    let (pool, _temp_dir) = setup_test_db().await;
    let manager = TagManager::new(pool.clone(), TagManagerConfig::default()).await.unwrap();
    let correction_service = TagCorrectionService::new(pool.clone());
    
    // Create tag and file
    let tag = manager.create_tag("RejectMe".to_string(), TagType::AutoGenerated, None, None, None).await.unwrap();
    let file_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO files (id, path, filename, extension, file_type, size_bytes, content_hash, created_at, modified_at, indexed_at, index_status, privacy_level, is_excluded)
        VALUES (?, '/test/reject_file.txt', 'reject_file.txt', 'txt', 'Text', 100, 'hash000', datetime('now'), datetime('now'), datetime('now'), 'Indexed', 'Normal', 0)
        "#
    )
    .bind(file_id.to_string())
    .execute(&pool)
    .await
    .unwrap();
    
    // Add AI-generated tag
    manager.add_tag_to_file(file_id, tag.id, TagSource::AIGenerated, Some(0.6)).await.unwrap();
    
    // Reject the tag
    let result = correction_service.execute(
        TagCommand::RejectTag { file_id, tag_id: tag.id, block_similar: false },
        &manager,
    ).await.unwrap();
    
    assert!(result.success);
    
    // Verify rejection
    let relation = manager.get_file_tag_relation(file_id, tag.id).await.unwrap().unwrap();
    assert!(!relation.is_confirmed);
    assert!(relation.is_rejected);
}

// ============================================================================
// Property-Based Tests
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]
    
    /// **Feature: neural-fs-core, Property 8: Tag Assignment Completeness**
    /// *For any* successfully indexed file, the Tag_Manager SHALL assign at least one tag.
    /// **Validates: Requirements 5.1**
    #[test]
    fn prop_tag_assignment_completeness(
        filename in "[a-zA-Z0-9_]{1,20}",
        extension in prop_oneof!["txt", "pdf", "jpg", "rs", "py", "doc", "mp4", "zip"]
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (pool, _temp_dir) = setup_test_db().await;
            let manager = TagManager::new(pool.clone(), TagManagerConfig::default()).await.unwrap();
            
            // Create a file entry
            let file_id = Uuid::now_v7();
            let full_filename = format!("{}.{}", filename, extension);
            sqlx::query(
                r#"
                INSERT INTO files (id, path, filename, extension, file_type, size_bytes, content_hash, created_at, modified_at, indexed_at, index_status, privacy_level, is_excluded)
                VALUES (?, ?, ?, ?, 'Document', 100, 'hash', datetime('now'), datetime('now'), datetime('now'), 'Indexed', 'Normal', 0)
                "#
            )
            .bind(file_id.to_string())
            .bind(format!("/test/{}", full_filename))
            .bind(&full_filename)
            .bind(&extension)
            .execute(&pool)
            .await
            .unwrap();
            
            // Auto-tag the file
            let result = manager.auto_tag_file(
                file_id,
                &full_filename,
                &extension,
                None,
            ).await.unwrap();
            
            // Property: At least one tag should be assigned
            prop_assert!(!result.assigned_tags.is_empty(), 
                "File {} should have at least one tag assigned", full_filename);
            
            // Verify tags are actually in the database
            let file_tags = manager.get_file_tags(file_id).await.unwrap();
            prop_assert!(!file_tags.is_empty(),
                "File {} should have tags in database", full_filename);
        });
    }
    
    /// **Feature: neural-fs-core, Property 9: Tag Hierarchy Depth Bound**
    /// *For any* tag in the system, the path from root to that tag SHALL have at most 3 levels.
    /// **Validates: Requirements 5.7**
    #[test]
    fn prop_tag_hierarchy_depth_bound(
        tag_names in prop::collection::vec("[a-zA-Z0-9_]{1,15}", 1..10)
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (pool, _temp_dir) = setup_test_db().await;
            let manager = TagManager::new(pool.clone(), TagManagerConfig::default()).await.unwrap();
            
            let mut created_tags = Vec::new();
            let mut parent_id: Option<Uuid> = None;
            
            // Try to create a chain of tags
            for (i, name) in tag_names.iter().enumerate() {
                let unique_name = format!("{}_{}", name, i);
                let result = manager.create_tag(
                    unique_name.clone(),
                    TagType::Category,
                    parent_id,
                    None,
                    None,
                ).await;
                
                match result {
                    Ok(tag) => {
                        created_tags.push(tag.id);
                        parent_id = Some(tag.id);
                    }
                    Err(TagError::HierarchyDepthExceeded { .. }) => {
                        // This is expected when we exceed the depth limit
                        break;
                    }
                    Err(e) => {
                        panic!("Unexpected error: {:?}", e);
                    }
                }
            }
            
            // Property: All created tags should have depth <= 2 (3 levels: 0, 1, 2)
            for tag_id in &created_tags {
                let depth = manager.hierarchy().get_depth(*tag_id).await.unwrap();
                prop_assert!(depth <= 2, 
                    "Tag depth {} exceeds maximum allowed depth of 2", depth);
            }
            
            // Property: The hierarchy should have at most 3 levels
            let stats = manager.hierarchy().get_stats().await;
            prop_assert!(stats.max_depth <= 2,
                "Hierarchy max depth {} exceeds limit of 2", stats.max_depth);
        });
    }
    
    /// **Feature: neural-fs-core, Property 24: Sensitive Tag Confirmation Requirement**
    /// *For any* AI-generated tag that matches sensitive patterns, the tag SHALL be marked
    /// as requiring user confirmation before being used in search ranking.
    /// **Validates: Requirements 5.5, 13.4, UI/UX Design**
    #[test]
    fn prop_sensitive_tag_confirmation_requirement(
        base_name in "[a-zA-Z]{1,10}",
        sensitive_keyword in prop_oneof![
            "personal", "private", "confidential", "secret",
            "bank", "account", "tax", "salary",
            "medical", "health", "diagnosis", "prescription",
            "legal", "contract", "nda", "proprietary"
        ]
    ) {
        let detector = SensitiveTagDetector::new();
        
        // Create a tag name that includes a sensitive keyword
        let sensitive_tag_name = format!("{} {}", base_name, sensitive_keyword);
        
        // Property: Tags with sensitive keywords should be detected
        let sensitivity = detector.check_sensitivity(&sensitive_tag_name);
        prop_assert!(sensitivity != SensitivityLevel::None,
            "Tag '{}' with sensitive keyword '{}' should be detected as sensitive",
            sensitive_tag_name, sensitive_keyword);
        
        // Property: Analysis should indicate confirmation is required
        let analysis = detector.analyze(&sensitive_tag_name);
        prop_assert!(analysis.requires_confirmation,
            "Tag '{}' should require confirmation", sensitive_tag_name);
        
        // Property: At least one pattern should match
        prop_assert!(!analysis.matched_patterns.is_empty(),
            "Tag '{}' should match at least one sensitive pattern", sensitive_tag_name);
    }
}

// Additional property tests for edge cases

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]
    
    /// Property: Non-sensitive tags should not require confirmation
    #[test]
    fn prop_non_sensitive_tags_no_confirmation(
        tag_name in "[a-zA-Z]{1,10}"
    ) {
        let detector = SensitiveTagDetector::new();
        
        // Filter out names that happen to contain sensitive keywords
        let sensitive_keywords = ["personal", "private", "bank", "medical", "legal", "secret", "tax", "health"];
        let tag_lower = tag_name.to_lowercase();
        let contains_sensitive = sensitive_keywords.iter().any(|kw| tag_lower.contains(kw));
        
        if !contains_sensitive {
            let sensitivity = detector.check_sensitivity(&tag_name);
            prop_assert_eq!(sensitivity, SensitivityLevel::None,
                "Tag '{}' without sensitive keywords should not be sensitive", tag_name);
        }
    }
    
    /// Property: Tag names should be validated
    #[test]
    fn prop_tag_name_validation(
        name in "[ ]{0,5}|[a-zA-Z0-9_]{101,150}"
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (pool, _temp_dir) = setup_test_db().await;
            let manager = TagManager::new(pool, TagManagerConfig::default()).await.unwrap();
            
            let result = manager.create_tag(
                name.clone(),
                TagType::Custom,
                None,
                None,
                None,
            ).await;
            
            // Empty or too long names should fail
            if name.trim().is_empty() || name.len() > 100 {
                prop_assert!(matches!(result, Err(TagError::InvalidTagName { .. })),
                    "Invalid tag name '{}' should be rejected", name);
            }
        });
    }
}
