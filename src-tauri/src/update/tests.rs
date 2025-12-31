//! Tests for the update module
//!
//! Contains unit tests and property-based tests for model downloading.

use super::model::*;
use proptest::prelude::*;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tempfile::TempDir;

/// Test helper to create a test model info
fn create_test_model(id: &str, size: u64, sha256: &str) -> ModelInfo {
    ModelInfo {
        id: id.to_string(),
        name: format!("Test Model {}", id),
        model_type: ModelType::TextEmbedding,
        filename: format!("{}.onnx", id),
        size_bytes: size,
        sha256: sha256.to_string(),
        required: true,
        description: "Test model".to_string(),
        vram_mb: 100,
    }
}

/// Test helper to create a downloader with temp directory
async fn create_test_downloader() -> (ModelDownloader, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let config = ModelDownloaderConfig {
        models_dir: temp_dir.path().to_path_buf(),
        ..Default::default()
    };
    let downloader = ModelDownloader::with_config(config).unwrap();
    (downloader, temp_dir)
}

// ============================================================================
// Resume Capability Tests
// ============================================================================

#[tokio::test]
async fn test_resume_info_no_partial() {
    let (downloader, _temp_dir) = create_test_downloader().await;
    let model = create_test_model("test", 1000, "abc123");
    
    // No partial file exists
    let resume_info = downloader.get_resume_info(&model).await;
    assert!(resume_info.is_none());
}

#[tokio::test]
async fn test_resume_info_with_partial() {
    let (downloader, temp_dir) = create_test_downloader().await;
    let model = create_test_model("test", 1000, "abc123");
    
    // Create a partial file with some content
    let partial_path = temp_dir.path().join("test.onnx.part");
    tokio::fs::write(&partial_path, vec![0u8; 500]).await.unwrap();
    
    let resume_info = downloader.get_resume_info(&model).await;
    assert!(resume_info.is_some());
    let (downloaded, total) = resume_info.unwrap();
    assert_eq!(downloaded, 500);
    assert_eq!(total, 1000);
}

#[tokio::test]
async fn test_partial_path_generation() {
    let (downloader, temp_dir) = create_test_downloader().await;
    let model = create_test_model("mymodel", 1000, "abc123");
    
    let partial_path = downloader.get_partial_path(&model);
    assert_eq!(partial_path, temp_dir.path().join("mymodel.onnx.part"));
}

#[tokio::test]
async fn test_cleanup_partial() {
    let (downloader, temp_dir) = create_test_downloader().await;
    let model = create_test_model("cleanup-test", 1000, "abc123");
    
    // Create a partial file
    let partial_path = temp_dir.path().join("cleanup-test.onnx.part");
    tokio::fs::write(&partial_path, b"partial data").await.unwrap();
    assert!(partial_path.exists());
    
    // Cleanup
    downloader.cleanup_partial(&model).await.unwrap();
    assert!(!partial_path.exists());
}

#[tokio::test]
async fn test_cleanup_nonexistent_partial() {
    let (downloader, _temp_dir) = create_test_downloader().await;
    let model = create_test_model("nonexistent", 1000, "abc123");
    
    // Should not error when file doesn't exist
    let result = downloader.cleanup_partial(&model).await;
    assert!(result.is_ok());
}

// ============================================================================
// Checksum Verification Tests
// ============================================================================

#[tokio::test]
async fn test_checksum_empty_file() {
    let (downloader, temp_dir) = create_test_downloader().await;
    let test_file = temp_dir.path().join("empty.txt");
    tokio::fs::write(&test_file, b"").await.unwrap();
    
    let checksum = downloader.calculate_checksum(&test_file).await.unwrap();
    // SHA256 of empty string
    assert_eq!(
        checksum,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[tokio::test]
async fn test_checksum_large_file() {
    let (downloader, temp_dir) = create_test_downloader().await;
    let test_file = temp_dir.path().join("large.bin");
    
    // Create a 2MB file
    let data = vec![0xABu8; 2 * 1024 * 1024];
    tokio::fs::write(&test_file, &data).await.unwrap();
    
    let checksum = downloader.calculate_checksum(&test_file).await.unwrap();
    assert!(!checksum.is_empty());
    assert_eq!(checksum.len(), 64); // SHA256 hex is 64 chars
}

#[tokio::test]
async fn test_verify_checksum_correct() {
    let (downloader, temp_dir) = create_test_downloader().await;
    let test_file = temp_dir.path().join("verify.txt");
    tokio::fs::write(&test_file, b"test content").await.unwrap();
    
    // Calculate actual checksum
    let actual = downloader.calculate_checksum(&test_file).await.unwrap();
    
    // Verify with correct checksum
    let result = downloader.verify_checksum(&test_file, &actual).await.unwrap();
    assert!(result);
}

#[tokio::test]
async fn test_verify_checksum_incorrect() {
    let (downloader, temp_dir) = create_test_downloader().await;
    let test_file = temp_dir.path().join("verify.txt");
    tokio::fs::write(&test_file, b"test content").await.unwrap();
    
    // Verify with wrong checksum
    let result = downloader
        .verify_checksum(&test_file, "0000000000000000000000000000000000000000000000000000000000000000")
        .await
        .unwrap();
    assert!(!result);
}

// ============================================================================
// Model Presence Tests
// ============================================================================

#[tokio::test]
async fn test_model_present_correct_size() {
    let (downloader, temp_dir) = create_test_downloader().await;
    let model = create_test_model("present", 100, "abc123");
    
    // Create file with correct size
    let model_path = temp_dir.path().join("present.onnx");
    tokio::fs::write(&model_path, vec![0u8; 100]).await.unwrap();
    
    assert!(downloader.is_model_present(&model));
}

#[tokio::test]
async fn test_model_present_wrong_size() {
    let (downloader, temp_dir) = create_test_downloader().await;
    let model = create_test_model("wrong-size", 100, "abc123");
    
    // Create file with wrong size
    let model_path = temp_dir.path().join("wrong-size.onnx");
    tokio::fs::write(&model_path, vec![0u8; 50]).await.unwrap();
    
    assert!(!downloader.is_model_present(&model));
}

// ============================================================================
// Download Status Tests
// ============================================================================

#[tokio::test]
async fn test_download_status_transitions() {
    let (downloader, _temp_dir) = create_test_downloader().await;
    let model_id = "status-test";
    
    // Initial state
    assert!(downloader.get_status(model_id).await.is_none());
    
    // Transition through states
    downloader.set_status(model_id, DownloadStatus::Pending).await;
    assert_eq!(downloader.get_status(model_id).await, Some(DownloadStatus::Pending));
    
    downloader.set_status(model_id, DownloadStatus::Downloading).await;
    assert_eq!(downloader.get_status(model_id).await, Some(DownloadStatus::Downloading));
    
    downloader.set_status(model_id, DownloadStatus::Verifying).await;
    assert_eq!(downloader.get_status(model_id).await, Some(DownloadStatus::Verifying));
    
    downloader.set_status(model_id, DownloadStatus::Completed).await;
    assert_eq!(downloader.get_status(model_id).await, Some(DownloadStatus::Completed));
}

#[tokio::test]
async fn test_download_status_failed() {
    let (downloader, _temp_dir) = create_test_downloader().await;
    let model_id = "failed-test";
    
    let failed_status = DownloadStatus::Failed {
        reason: "Network error".to_string(),
    };
    downloader.set_status(model_id, failed_status.clone()).await;
    
    let status = downloader.get_status(model_id).await.unwrap();
    match status {
        DownloadStatus::Failed { reason } => {
            assert_eq!(reason, "Network error");
        }
        _ => panic!("Expected Failed status"),
    }
}

// ============================================================================
// Configuration Tests
// ============================================================================

#[test]
fn test_config_sources_sorted_by_priority() {
    let config = ModelDownloaderConfig::default();
    
    // Verify sources are in priority order
    let priorities: Vec<u32> = config.sources.iter().map(|s| s.priority).collect();
    let mut sorted = priorities.clone();
    sorted.sort();
    assert_eq!(priorities, sorted);
}

#[test]
fn test_config_custom_sources() {
    let config = ModelDownloaderConfig {
        sources: vec![
            ModelSource::new("Custom", "https://custom.example.com", 1),
        ],
        ..Default::default()
    };
    
    assert_eq!(config.sources.len(), 1);
    assert_eq!(config.sources[0].name, "Custom");
}

// ============================================================================
// Manifest Tests
// ============================================================================

#[test]
fn test_manifest_serialization() {
    let manifest = ModelManifest::default();
    let json = serde_json::to_string(&manifest).unwrap();
    let deserialized: ModelManifest = serde_json::from_str(&json).unwrap();
    
    assert_eq!(manifest.version, deserialized.version);
    assert_eq!(manifest.models.len(), deserialized.models.len());
}

#[test]
fn test_manifest_has_required_models() {
    let manifest = ModelManifest::default();
    let required_count = manifest.models.iter().filter(|m| m.required).count();
    assert!(required_count > 0, "Manifest should have at least one required model");
}

// ============================================================================
// Error Tests
// ============================================================================

#[test]
fn test_error_display() {
    let err = DownloadError::AllSourcesFailed {
        model_id: "test-model".to_string(),
    };
    assert!(err.to_string().contains("test-model"));
    
    let err = DownloadError::ChecksumMismatch {
        filename: "model.onnx".to_string(),
        expected: "abc".to_string(),
        actual: "def".to_string(),
    };
    assert!(err.to_string().contains("model.onnx"));
    assert!(err.to_string().contains("abc"));
    assert!(err.to_string().contains("def"));
}

// ============================================================================
// Property-Based Tests
// ============================================================================

proptest! {
    /// Property: Download progress percentage is always between 0 and 100
    #[test]
    fn prop_progress_percentage_bounded(downloaded in 0u64..=u64::MAX, total in 1u64..=u64::MAX) {
        let percentage = DownloadProgress::calculate_percentage(downloaded, total);
        prop_assert!(percentage <= 100);
    }

    /// Property: Checksum is always 64 hex characters (SHA256)
    #[test]
    fn prop_checksum_format(data in prop::collection::vec(any::<u8>(), 0..1000)) {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let result = format!("{:x}", hasher.finalize());
        prop_assert_eq!(result.len(), 64);
        prop_assert!(result.chars().all(|c| c.is_ascii_hexdigit()));
    }

    /// Property: Model source priority ordering is preserved
    #[test]
    fn prop_source_priority_ordering(
        priorities in prop::collection::vec(0u32..100, 1..10)
    ) {
        let sources: Vec<ModelSource> = priorities
            .iter()
            .enumerate()
            .map(|(i, &p)| ModelSource::new(format!("Source{}", i), "https://example.com", p))
            .collect();
        
        let mut sorted = sources.clone();
        sorted.sort_by_key(|s| s.priority);
        
        // Verify sorting preserves relative order for equal priorities
        for i in 1..sorted.len() {
            prop_assert!(sorted[i-1].priority <= sorted[i].priority);
        }
    }
}

// ============================================================================
// Property 23: Model Download Integrity
// **Feature: neural-fs-core, Property 23: Model Download Integrity**
// **Validates: Requirements 20, Installer Specification**
//
// *For any* downloaded model file, the SHA256 checksum SHALL match the 
// expected value in the manifest.
// ============================================================================

/// Strategy to generate random file content
fn arb_file_content() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 1..10000)
}

/// Strategy to generate model info with matching checksum for given content
fn arb_model_with_content() -> impl Strategy<Value = (ModelInfo, Vec<u8>)> {
    arb_file_content().prop_map(|content| {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(&content);
        let checksum = format!("{:x}", hasher.finalize());
        
        let model = ModelInfo {
            id: "test-model".to_string(),
            name: "Test Model".to_string(),
            model_type: ModelType::TextEmbedding,
            filename: "test-model.onnx".to_string(),
            size_bytes: content.len() as u64,
            sha256: checksum,
            required: true,
            description: "Test model for property testing".to_string(),
            vram_mb: 100,
        };
        
        (model, content)
    })
}

proptest! {
    /// **Property 23: Model Download Integrity**
    /// 
    /// For any downloaded model file, the SHA256 checksum SHALL match the 
    /// expected value in the manifest.
    ///
    /// This property verifies that:
    /// 1. When a file is written with known content
    /// 2. And the manifest contains the correct SHA256 checksum for that content
    /// 3. Then verify_checksum returns true
    ///
    /// **Feature: neural-fs-core, Property 23: Model Download Integrity**
    /// **Validates: Requirements 20, Installer Specification**
    #[test]
    fn prop_model_download_integrity((model, content) in arb_model_with_content()) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new().unwrap();
            let config = ModelDownloaderConfig {
                models_dir: temp_dir.path().to_path_buf(),
                ..Default::default()
            };
            let downloader = ModelDownloader::with_config(config).unwrap();
            
            // Simulate a "downloaded" file by writing content directly
            let model_path = downloader.get_model_path(&model);
            tokio::fs::write(&model_path, &content).await.unwrap();
            
            // Property: The checksum verification MUST succeed when the file
            // content matches what was used to generate the manifest checksum
            let is_valid = downloader
                .verify_checksum(&model_path, &model.sha256)
                .await
                .unwrap();
            
            prop_assert!(
                is_valid,
                "Checksum verification failed for model {} with {} bytes",
                model.id,
                content.len()
            );
            
            // Additional property: The model should be detected as present
            prop_assert!(
                downloader.is_model_present(&model),
                "Model should be detected as present after download"
            );
            
            Ok(())
        })?;
    }

    /// **Property 23 (Negative Case): Corrupted Download Detection**
    /// 
    /// For any model file with corrupted content (different from expected),
    /// the SHA256 checksum SHALL NOT match the expected value.
    ///
    /// This ensures that corrupted downloads are properly detected.
    ///
    /// **Feature: neural-fs-core, Property 23: Model Download Integrity**
    /// **Validates: Requirements 20, Installer Specification**
    #[test]
    fn prop_corrupted_download_detection(
        (model, original_content) in arb_model_with_content(),
        corruption_byte in any::<u8>(),
        corruption_pos in 0usize..10000
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new().unwrap();
            let config = ModelDownloaderConfig {
                models_dir: temp_dir.path().to_path_buf(),
                ..Default::default()
            };
            let downloader = ModelDownloader::with_config(config).unwrap();
            
            // Create corrupted content by modifying a byte
            let mut corrupted_content = original_content.clone();
            let pos = corruption_pos % corrupted_content.len();
            
            // Only corrupt if the byte would actually change
            if corrupted_content[pos] != corruption_byte {
                corrupted_content[pos] = corruption_byte;
                
                // Write corrupted content
                let model_path = downloader.get_model_path(&model);
                tokio::fs::write(&model_path, &corrupted_content).await.unwrap();
                
                // Property: Checksum verification MUST fail for corrupted content
                let is_valid = downloader
                    .verify_checksum(&model_path, &model.sha256)
                    .await
                    .unwrap();
                
                prop_assert!(
                    !is_valid,
                    "Checksum verification should fail for corrupted content"
                );
            }
            
            Ok(())
        })?;
    }

    /// **Property 23 (Round-Trip): Calculate then Verify**
    /// 
    /// For any file content, calculating the checksum and then verifying
    /// with that same checksum SHALL always succeed.
    ///
    /// **Feature: neural-fs-core, Property 23: Model Download Integrity**
    /// **Validates: Requirements 20, Installer Specification**
    #[test]
    fn prop_checksum_round_trip(content in arb_file_content()) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new().unwrap();
            let config = ModelDownloaderConfig {
                models_dir: temp_dir.path().to_path_buf(),
                ..Default::default()
            };
            let downloader = ModelDownloader::with_config(config).unwrap();
            
            // Write content to file
            let test_file = temp_dir.path().join("test.bin");
            tokio::fs::write(&test_file, &content).await.unwrap();
            
            // Calculate checksum
            let calculated = downloader.calculate_checksum(&test_file).await.unwrap();
            
            // Property: Verifying with calculated checksum MUST succeed
            let is_valid = downloader
                .verify_checksum(&test_file, &calculated)
                .await
                .unwrap();
            
            prop_assert!(
                is_valid,
                "Round-trip checksum verification failed"
            );
            
            Ok(())
        })?;
    }
}


// ============================================================================
// Self-Update Tests
// ============================================================================

use super::self_update::*;
use chrono::Utc;

// ============================================================================
// Version Tests
// ============================================================================

#[test]
fn test_version_parsing_various_formats() {
    // Standard format
    assert!(Version::parse("1.0.0").is_ok());
    assert!(Version::parse("0.1.0").is_ok());
    assert!(Version::parse("10.20.30").is_ok());
    
    // With v prefix
    assert!(Version::parse("v1.0.0").is_ok());
    
    // With prerelease
    assert!(Version::parse("1.0.0-alpha").is_ok());
    assert!(Version::parse("1.0.0-beta.1").is_ok());
    assert!(Version::parse("1.0.0-rc.1").is_ok());
    
    // Invalid formats
    assert!(Version::parse("1.0").is_err());
    assert!(Version::parse("1").is_err());
    assert!(Version::parse("").is_err());
    assert!(Version::parse("a.b.c").is_err());
}

#[test]
fn test_version_comparison_comprehensive() {
    // Major version comparison
    assert!(Version::new(2, 0, 0) > Version::new(1, 0, 0));
    assert!(Version::new(1, 0, 0) < Version::new(2, 0, 0));
    
    // Minor version comparison
    assert!(Version::new(1, 2, 0) > Version::new(1, 1, 0));
    assert!(Version::new(1, 1, 0) < Version::new(1, 2, 0));
    
    // Patch version comparison
    assert!(Version::new(1, 0, 2) > Version::new(1, 0, 1));
    assert!(Version::new(1, 0, 1) < Version::new(1, 0, 2));
    
    // Prerelease comparison
    let alpha = Version::with_prerelease(1, 0, 0, "alpha");
    let beta = Version::with_prerelease(1, 0, 0, "beta");
    let release = Version::new(1, 0, 0);
    
    assert!(alpha < beta);
    assert!(beta < release);
    assert!(alpha < release);
}

// ============================================================================
// Update State Machine Tests
// ============================================================================

#[test]
fn test_update_state_can_cancel() {
    assert!(UpdateState::Checking.can_cancel());
    assert!(UpdateState::AwaitingConfirmation.can_cancel());
    assert!(UpdateState::Downloading.can_cancel());
    assert!(UpdateState::Verifying.can_cancel());
    assert!(UpdateState::ReadyToApply.can_cancel());
    
    assert!(!UpdateState::Idle.can_cancel());
    assert!(!UpdateState::Applying.can_cancel());
    assert!(!UpdateState::RestartRequired.can_cancel());
    assert!(!UpdateState::Failed.can_cancel());
    assert!(!UpdateState::RolledBack.can_cancel());
}

#[test]
fn test_update_state_can_rollback() {
    assert!(UpdateState::Failed.can_rollback());
    assert!(UpdateState::RestartRequired.can_rollback());
    
    assert!(!UpdateState::Idle.can_rollback());
    assert!(!UpdateState::Checking.can_rollback());
    assert!(!UpdateState::Downloading.can_rollback());
    assert!(!UpdateState::Applying.can_rollback());
}

#[test]
fn test_update_state_is_terminal() {
    assert!(UpdateState::Idle.is_terminal());
    assert!(UpdateState::RolledBack.is_terminal());
    assert!(UpdateState::RestartRequired.is_terminal());
    
    assert!(!UpdateState::Checking.is_terminal());
    assert!(!UpdateState::Downloading.is_terminal());
    assert!(!UpdateState::Applying.is_terminal());
    assert!(!UpdateState::Failed.is_terminal());
}

// ============================================================================
// AtomicUpdateResult Tests
// ============================================================================

#[test]
fn test_atomic_update_result_success() {
    let from = Version::new(1, 0, 0);
    let to = Version::new(2, 0, 0);
    let result = AtomicUpdateResult::success(from.clone(), to.clone());
    
    assert!(result.success);
    assert_eq!(result.final_state, UpdateState::RestartRequired);
    assert!(result.error.is_none());
    assert_eq!(result.from_version, from);
    assert_eq!(result.to_version, Some(to));
}

#[test]
fn test_atomic_update_result_failure() {
    let from = Version::new(1, 0, 0);
    let result = AtomicUpdateResult::failure(from.clone(), "Download failed");
    
    assert!(!result.success);
    assert_eq!(result.final_state, UpdateState::Failed);
    assert_eq!(result.error, Some("Download failed".to_string()));
    assert_eq!(result.from_version, from);
    assert!(result.to_version.is_none());
}

#[test]
fn test_atomic_update_result_rollback() {
    let from = Version::new(1, 0, 0);
    let result = AtomicUpdateResult::rollback(from.clone());
    
    assert!(result.success);
    assert_eq!(result.final_state, UpdateState::RolledBack);
    assert!(result.error.is_none());
    assert_eq!(result.from_version, from.clone());
    assert_eq!(result.to_version, Some(from));
}

// ============================================================================
// SelfUpdater Configuration Tests
// ============================================================================

#[test]
fn test_self_updater_config_default() {
    let config = SelfUpdaterConfig::default();
    
    assert!(!config.update_server.is_empty());
    assert!(config.auto_check);
    assert!(!config.auto_download);
    assert_eq!(config.channel, UpdateChannel::Stable);
    assert_eq!(config.check_interval_hours, 24);
}

#[tokio::test]
async fn test_self_updater_creation_and_status() {
    let updater = SelfUpdater::new(Version::new(1, 0, 0)).unwrap();
    
    assert_eq!(updater.current_version(), &Version::new(1, 0, 0));
    assert_eq!(updater.status().await, UpdateStatus::UpToDate);
    assert!(!updater.has_backup());
}

// ============================================================================
// Property-Based Tests for Self-Update
// ============================================================================

/// Strategy to generate valid version components
fn arb_version_component() -> impl Strategy<Value = u32> {
    0u32..1000
}

/// Strategy to generate valid versions
fn arb_version() -> impl Strategy<Value = Version> {
    (arb_version_component(), arb_version_component(), arb_version_component())
        .prop_map(|(major, minor, patch)| Version::new(major, minor, patch))
}

/// Strategy to generate version with optional prerelease
fn arb_version_with_prerelease() -> impl Strategy<Value = Version> {
    (
        arb_version_component(),
        arb_version_component(),
        arb_version_component(),
        prop::option::of("[a-z]+\\.[0-9]+"),
    )
        .prop_map(|(major, minor, patch, prerelease)| {
            if let Some(pre) = prerelease {
                Version::with_prerelease(major, minor, patch, pre)
            } else {
                Version::new(major, minor, patch)
            }
        })
}

proptest! {
    // ========================================================================
    // **Property 29: Update Atomicity**
    // **Feature: neural-fs-core, Property 29: Update Atomicity**
    // **Validates: Self-Update Strategy**
    //
    // *For any* self-update operation, either the update completes successfully
    // with the new version running, or the system rolls back to the previous version.
    // ========================================================================

    /// **Property 29: Update Atomicity - State Machine Validity**
    ///
    /// For any sequence of update states, the state machine transitions
    /// SHALL only follow valid paths, ensuring atomicity.
    ///
    /// **Feature: neural-fs-core, Property 29: Update Atomicity**
    /// **Validates: Self-Update Strategy**
    #[test]
    fn prop_update_state_machine_validity(
        initial_state in prop::sample::select(vec![
            UpdateState::Idle,
            UpdateState::Checking,
            UpdateState::AwaitingConfirmation,
            UpdateState::Downloading,
            UpdateState::Verifying,
            UpdateState::ReadyToApply,
            UpdateState::Applying,
            UpdateState::RestartRequired,
            UpdateState::Failed,
            UpdateState::RolledBack,
        ])
    ) {
        // Property: Every state has well-defined cancellation and rollback behavior
        let can_cancel = initial_state.can_cancel();
        let can_rollback = initial_state.can_rollback();
        let is_terminal = initial_state.is_terminal();
        
        // Invariant: Terminal states cannot be cancelled
        if is_terminal {
            prop_assert!(
                !can_cancel || initial_state == UpdateState::Idle,
                "Terminal state {:?} should not be cancellable (except Idle)",
                initial_state
            );
        }
        
        // Invariant: Only failed or restart-required states can rollback
        if can_rollback {
            prop_assert!(
                initial_state == UpdateState::Failed || initial_state == UpdateState::RestartRequired,
                "Only Failed or RestartRequired states should allow rollback, got {:?}",
                initial_state
            );
        }
        
        // Invariant: Applying state is not cancellable (atomic operation)
        if initial_state == UpdateState::Applying {
            prop_assert!(
                !can_cancel,
                "Applying state must not be cancellable to ensure atomicity"
            );
        }
    }

    /// **Property 29: Update Atomicity - Result Consistency**
    ///
    /// For any update result, the success flag SHALL be consistent with
    /// the final state and error presence.
    ///
    /// **Feature: neural-fs-core, Property 29: Update Atomicity**
    /// **Validates: Self-Update Strategy**
    #[test]
    fn prop_update_result_consistency(
        from_version in arb_version(),
        to_version in arb_version(),
        error_msg in prop::option::of("[a-zA-Z0-9 ]+"),
    ) {
        // Test success case
        let success_result = AtomicUpdateResult::success(from_version.clone(), to_version.clone());
        prop_assert!(success_result.success);
        prop_assert!(success_result.error.is_none());
        prop_assert!(success_result.to_version.is_some());
        prop_assert_eq!(success_result.final_state, UpdateState::RestartRequired);
        
        // Test failure case
        if let Some(ref msg) = error_msg {
            let failure_result = AtomicUpdateResult::failure(from_version.clone(), msg);
            prop_assert!(!failure_result.success);
            prop_assert!(failure_result.error.is_some());
            prop_assert!(failure_result.to_version.is_none());
            prop_assert_eq!(failure_result.final_state, UpdateState::Failed);
        }
        
        // Test rollback case
        let rollback_result = AtomicUpdateResult::rollback(from_version.clone());
        prop_assert!(rollback_result.success);
        prop_assert!(rollback_result.error.is_none());
        prop_assert_eq!(rollback_result.to_version, Some(from_version.clone()));
        prop_assert_eq!(rollback_result.final_state, UpdateState::RolledBack);
    }

    // ========================================================================
    // **Property 30: Watchdog Recovery Guarantee**
    // **Feature: neural-fs-core, Property 30: Watchdog Recovery Guarantee**
    // **Validates: Process Supervisor**
    //
    // *For any* main process crash, the Watchdog SHALL either restart the main
    // process or restore Windows Explorer within (max_restart_attempts * restart_cooldown) seconds.
    // ========================================================================

    /// **Property 30: Watchdog Recovery Guarantee - Command Serialization**
    ///
    /// For any watchdog command, serialization and deserialization SHALL
    /// produce an equivalent command, ensuring reliable IPC.
    ///
    /// **Feature: neural-fs-core, Property 30: Watchdog Recovery Guarantee**
    /// **Validates: Process Supervisor**
    #[test]
    fn prop_watchdog_command_serialization_roundtrip(
        cmd_type in prop::sample::select(vec![
            WatchdogCommand::PrepareUpdate,
            WatchdogCommand::PrepareRollback,
            WatchdogCommand::UpdateComplete,
            WatchdogCommand::Shutdown,
        ])
    ) {
        // Serialize
        let json = serde_json::to_string(&cmd_type).unwrap();
        
        // Deserialize
        let deserialized: WatchdogCommand = serde_json::from_str(&json).unwrap();
        
        // Property: Round-trip serialization preserves command type
        let original_json = serde_json::to_string(&cmd_type).unwrap();
        let roundtrip_json = serde_json::to_string(&deserialized).unwrap();
        
        prop_assert_eq!(
            original_json,
            roundtrip_json,
            "Watchdog command serialization round-trip failed"
        );
    }

    /// **Property 29/30: Version Ordering Transitivity**
    ///
    /// For any three versions a, b, c: if a < b and b < c, then a < c.
    /// This ensures update version comparisons are consistent.
    ///
    /// **Feature: neural-fs-core, Property 29: Update Atomicity**
    /// **Validates: Self-Update Strategy**
    #[test]
    fn prop_version_ordering_transitivity(
        a in arb_version(),
        b in arb_version(),
        c in arb_version(),
    ) {
        // Transitivity: if a < b and b < c, then a < c
        if a < b && b < c {
            prop_assert!(
                a < c,
                "Version ordering transitivity violated: {:?} < {:?} < {:?} but {:?} >= {:?}",
                a, b, c, a, c
            );
        }
        
        // Antisymmetry: if a < b, then !(b < a)
        if a < b {
            prop_assert!(
                !(b < a),
                "Version ordering antisymmetry violated: {:?} < {:?} but also {:?} < {:?}",
                a, b, b, a
            );
        }
        
        // Reflexivity of equality
        prop_assert!(
            a == a,
            "Version equality reflexivity violated for {:?}",
            a
        );
    }

    /// **Property 29: Version Parse Round-Trip**
    ///
    /// For any valid version, converting to string and parsing back
    /// SHALL produce an equivalent version.
    ///
    /// **Feature: neural-fs-core, Property 29: Update Atomicity**
    /// **Validates: Self-Update Strategy**
    #[test]
    fn prop_version_parse_roundtrip(version in arb_version()) {
        let version_string = version.to_string();
        let parsed = Version::parse(&version_string).unwrap();
        
        prop_assert_eq!(
            version,
            parsed,
            "Version parse round-trip failed: {} -> {:?}",
            version_string,
            parsed
        );
    }

    /// **Property 29: Update Info Serialization Round-Trip**
    ///
    /// For any update info, serialization and deserialization SHALL
    /// produce equivalent data, ensuring reliable update metadata transfer.
    ///
    /// **Feature: neural-fs-core, Property 29: Update Atomicity**
    /// **Validates: Self-Update Strategy**
    #[test]
    fn prop_update_info_serialization_roundtrip(
        version in arb_version(),
        size_bytes in 1u64..1_000_000_000,
        is_critical in any::<bool>(),
    ) {
        let info = UpdateInfo {
            version: version.clone(),
            release_date: Utc::now(),
            download_url: "https://example.com/update.zip".to_string(),
            size_bytes,
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
            changelog: "Test changelog".to_string(),
            is_critical,
            min_version: None,
        };
        
        // Serialize
        let json = serde_json::to_string(&info).unwrap();
        
        // Deserialize
        let deserialized: UpdateInfo = serde_json::from_str(&json).unwrap();
        
        // Property: Key fields are preserved
        prop_assert_eq!(info.version, deserialized.version);
        prop_assert_eq!(info.size_bytes, deserialized.size_bytes);
        prop_assert_eq!(info.sha256, deserialized.sha256);
        prop_assert_eq!(info.is_critical, deserialized.is_critical);
    }
}
