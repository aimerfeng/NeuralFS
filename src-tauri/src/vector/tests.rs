//! Tests for the vector store module

use super::*;
use proptest::prelude::*;
use std::collections::HashMap;
use tempfile::TempDir;

/// Helper to create a test VectorStore with a temporary directory
async fn create_test_store(vector_size: u64) -> (VectorStore, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config = VectorStoreConfig::default()
        .with_storage_path(temp_dir.path().to_string_lossy().to_string())
        .with_vector_size(vector_size);
    
    let store = VectorStore::new(config).await.expect("Failed to create store");
    (store, temp_dir)
}

/// Generate a random vector of the given dimension
fn random_vector(dim: usize) -> Vec<f32> {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect()
}

/// Normalize a vector to unit length
fn normalize(v: &[f32]) -> Vec<f32> {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm == 0.0 {
        v.to_vec()
    } else {
        v.iter().map(|x| x / norm).collect()
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[tokio::test]
async fn test_vector_store_initialization() {
    let (store, _temp_dir) = create_test_store(384).await;
    
    assert!(store.is_initialized().await);
    assert_eq!(store.config().vector_size, 384);
    assert_eq!(store.count().await.unwrap(), 0);
}

#[tokio::test]
async fn test_upsert_and_get() {
    let (store, _temp_dir) = create_test_store(4).await;
    
    let vector = vec![1.0, 0.0, 0.0, 0.0];
    let point = VectorPoint::new(1, vector.clone())
        .with_file_id(uuid::Uuid::new_v4());
    
    let id = store.upsert(point).await.unwrap();
    assert_eq!(id, 1);
    
    let result = store.get(1).await.unwrap();
    assert!(result.is_some());
    
    let result = result.unwrap();
    assert_eq!(result.id, 1);
    assert_eq!(result.vector.unwrap(), vector);
}

#[tokio::test]
async fn test_batch_upsert() {
    let (store, _temp_dir) = create_test_store(4).await;
    
    let points: Vec<VectorPoint> = (1..=10)
        .map(|i| VectorPoint::new(i, vec![i as f32, 0.0, 0.0, 0.0]))
        .collect();
    
    let ids = store.upsert_batch(points).await.unwrap();
    assert_eq!(ids.len(), 10);
    assert_eq!(store.count().await.unwrap(), 10);
}

#[tokio::test]
async fn test_search_basic() {
    let (store, _temp_dir) = create_test_store(4).await;
    
    // Insert some vectors
    let points = vec![
        VectorPoint::new(1, vec![1.0, 0.0, 0.0, 0.0]),
        VectorPoint::new(2, vec![0.9, 0.1, 0.0, 0.0]),
        VectorPoint::new(3, vec![0.0, 1.0, 0.0, 0.0]),
        VectorPoint::new(4, vec![0.0, 0.0, 1.0, 0.0]),
    ];
    store.upsert_batch(points).await.unwrap();
    
    // Search for vectors similar to [1, 0, 0, 0]
    let query = vec![1.0, 0.0, 0.0, 0.0];
    let results = store.search(&query, 2, None).await.unwrap();
    
    assert_eq!(results.len(), 2);
    // First result should be the exact match
    assert_eq!(results[0].id, 1);
    // Second should be the closest
    assert_eq!(results[1].id, 2);
}

#[tokio::test]
async fn test_search_with_filter() {
    let (store, _temp_dir) = create_test_store(4).await;
    
    let file_id1 = uuid::Uuid::new_v4();
    let file_id2 = uuid::Uuid::new_v4();
    
    let points = vec![
        VectorPoint::new(1, vec![1.0, 0.0, 0.0, 0.0])
            .with_file_id(file_id1)
            .with_file_type("TextDocument"),
        VectorPoint::new(2, vec![0.9, 0.1, 0.0, 0.0])
            .with_file_id(file_id2)
            .with_file_type("Image"),
        VectorPoint::new(3, vec![0.8, 0.2, 0.0, 0.0])
            .with_file_id(file_id1)
            .with_file_type("TextDocument"),
    ];
    store.upsert_batch(points).await.unwrap();
    
    // Search with file type filter
    let query = vec![1.0, 0.0, 0.0, 0.0];
    let filter = SearchFilter::new().with_file_types(vec!["TextDocument".to_string()]);
    let results = store.search(&query, 10, Some(filter)).await.unwrap();
    
    assert_eq!(results.len(), 2);
    // All results should be TextDocument
    for result in &results {
        let file_type = result.payload.get("file_type").unwrap().as_str().unwrap();
        assert_eq!(file_type, "TextDocument");
    }
}

#[tokio::test]
async fn test_delete() {
    let (store, _temp_dir) = create_test_store(4).await;
    
    let point = VectorPoint::new(1, vec![1.0, 0.0, 0.0, 0.0]);
    store.upsert(point).await.unwrap();
    
    assert_eq!(store.count().await.unwrap(), 1);
    
    let deleted = store.delete(1).await.unwrap();
    assert!(deleted);
    
    assert_eq!(store.count().await.unwrap(), 0);
    assert!(store.get(1).await.unwrap().is_none());
}

#[tokio::test]
async fn test_delete_by_file_id() {
    let (store, _temp_dir) = create_test_store(4).await;
    
    let file_id = uuid::Uuid::new_v4();
    let other_file_id = uuid::Uuid::new_v4();
    
    let points = vec![
        VectorPoint::new(1, vec![1.0, 0.0, 0.0, 0.0]).with_file_id(file_id),
        VectorPoint::new(2, vec![0.0, 1.0, 0.0, 0.0]).with_file_id(file_id),
        VectorPoint::new(3, vec![0.0, 0.0, 1.0, 0.0]).with_file_id(other_file_id),
    ];
    store.upsert_batch(points).await.unwrap();
    
    assert_eq!(store.count().await.unwrap(), 3);
    
    let deleted = store.delete_by_file_id(file_id).await.unwrap();
    assert_eq!(deleted, 2);
    
    assert_eq!(store.count().await.unwrap(), 1);
}

#[tokio::test]
async fn test_invalid_dimension() {
    let (store, _temp_dir) = create_test_store(4).await;
    
    // Try to insert a vector with wrong dimension
    let point = VectorPoint::new(1, vec![1.0, 0.0]); // Only 2 dimensions
    let result = store.upsert(point).await;
    
    assert!(result.is_err());
    match result.unwrap_err() {
        VectorError::InvalidDimension { expected, actual } => {
            assert_eq!(expected, 4);
            assert_eq!(actual, 2);
        }
        _ => panic!("Expected InvalidDimension error"),
    }
}

#[tokio::test]
async fn test_clear() {
    let (store, _temp_dir) = create_test_store(4).await;
    
    let points: Vec<VectorPoint> = (1..=5)
        .map(|i| VectorPoint::new(i, vec![i as f32, 0.0, 0.0, 0.0]))
        .collect();
    store.upsert_batch(points).await.unwrap();
    
    assert_eq!(store.count().await.unwrap(), 5);
    
    let cleared = store.clear().await.unwrap();
    assert_eq!(cleared, 5);
    assert_eq!(store.count().await.unwrap(), 0);
}

#[tokio::test]
async fn test_lock_file_cleanup() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let storage_path = temp_dir.path();
    
    // Create some fake lock files
    std::fs::create_dir_all(storage_path.join("collection")).unwrap();
    std::fs::write(storage_path.join("storage.lock"), "").unwrap();
    std::fs::write(storage_path.join("collection").join(".lock"), "").unwrap();
    
    // Create the store - it should clean up the lock files
    let config = VectorStoreConfig::default()
        .with_storage_path(storage_path.to_string_lossy().to_string())
        .with_vector_size(4);
    
    let _store = VectorStore::new(config).await.expect("Failed to create store");
    
    // Lock files should be removed
    assert!(!storage_path.join("storage.lock").exists());
    assert!(!storage_path.join("collection").join(".lock").exists());
}

// ============================================================================
// Property Tests
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]
    
    /// **Feature: neural-fs-core, Property 4: Search Result Ordering**
    /// *For any* search response with multiple results, the results SHALL be sorted by score in descending order.
    /// **Validates: Requirements 2.2, 2.3**
    #[test]
    fn prop_search_results_ordered_by_score(
        num_vectors in 5usize..50,
        query_seed in any::<u64>(),
        limit in 1usize..20,
    ) {
        // Use tokio runtime for async test
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let dim = 16usize;
            let (store, _temp_dir) = create_test_store(dim as u64).await;
            
            // Generate random vectors
            use rand::SeedableRng;
            let mut rng = rand::rngs::StdRng::seed_from_u64(query_seed);
            
            let points: Vec<VectorPoint> = (1..=num_vectors as u64)
                .map(|i| {
                    let vector: Vec<f32> = (0..dim)
                        .map(|_| rand::Rng::gen_range(&mut rng, -1.0..1.0))
                        .collect();
                    VectorPoint::new(i, vector)
                })
                .collect();
            
            store.upsert_batch(points).await.unwrap();
            
            // Generate a random query vector
            let query: Vec<f32> = (0..dim)
                .map(|_| rand::Rng::gen_range(&mut rng, -1.0..1.0))
                .collect();
            
            // Search
            let results = store.search(&query, limit, None).await.unwrap();
            
            // Verify results are sorted by score in descending order
            for i in 1..results.len() {
                prop_assert!(
                    results[i - 1].score >= results[i].score,
                    "Results not sorted: score[{}]={} < score[{}]={}",
                    i - 1, results[i - 1].score, i, results[i].score
                );
            }
        });
    }
    
    /// **Feature: neural-fs-core, Property 17: Vector Database Serialization Round-Trip**
    /// *For any* VectorPoint, storing and retrieving should produce equivalent data.
    /// **Validates: Requirements 21**
    #[test]
    fn prop_vector_roundtrip(
        id in 1u64..1000,
        vector_values in proptest::collection::vec(-1.0f32..1.0f32, 16),
        file_id_bytes in any::<u128>(),
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (store, _temp_dir) = create_test_store(16).await;
            
            let file_id = uuid::Uuid::from_u128(file_id_bytes);
            let point = VectorPoint::new(id, vector_values.clone())
                .with_file_id(file_id)
                .with_file_type("TextDocument");
            
            // Store the vector
            store.upsert(point).await.unwrap();
            
            // Retrieve it
            let result = store.get(id).await.unwrap();
            prop_assert!(result.is_some(), "Vector not found after upsert");
            
            let result = result.unwrap();
            prop_assert_eq!(result.id, id);
            
            // Check vector values match
            let retrieved_vector = result.vector.unwrap();
            prop_assert_eq!(retrieved_vector.len(), vector_values.len());
            for (a, b) in retrieved_vector.iter().zip(vector_values.iter()) {
                prop_assert!(
                    (a - b).abs() < 1e-6,
                    "Vector values don't match: {} vs {}",
                    a, b
                );
            }
            
            // Check payload
            let retrieved_file_id = result.file_id();
            prop_assert_eq!(retrieved_file_id, Some(file_id));
        });
    }
    
    /// Property: Search limit is respected
    /// *For any* search with a limit, the number of results should not exceed the limit.
    #[test]
    fn prop_search_respects_limit(
        num_vectors in 10usize..100,
        limit in 1usize..50,
        seed in any::<u64>(),
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let dim = 8usize;
            let (store, _temp_dir) = create_test_store(dim as u64).await;
            
            use rand::SeedableRng;
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            
            let points: Vec<VectorPoint> = (1..=num_vectors as u64)
                .map(|i| {
                    let vector: Vec<f32> = (0..dim)
                        .map(|_| rand::Rng::gen_range(&mut rng, -1.0..1.0))
                        .collect();
                    VectorPoint::new(i, vector)
                })
                .collect();
            
            store.upsert_batch(points).await.unwrap();
            
            let query: Vec<f32> = (0..dim)
                .map(|_| rand::Rng::gen_range(&mut rng, -1.0..1.0))
                .collect();
            
            let results = store.search(&query, limit, None).await.unwrap();
            
            prop_assert!(
                results.len() <= limit,
                "Got {} results but limit was {}",
                results.len(), limit
            );
            
            // If we have enough vectors, we should get exactly `limit` results
            if num_vectors >= limit {
                prop_assert_eq!(
                    results.len(), limit,
                    "Expected {} results but got {}",
                    limit, results.len()
                );
            }
        });
    }
    
    /// Property: Delete removes vectors
    /// *For any* vector that is deleted, it should no longer be retrievable.
    #[test]
    fn prop_delete_removes_vector(
        id in 1u64..1000,
        vector_values in proptest::collection::vec(-1.0f32..1.0f32, 8),
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (store, _temp_dir) = create_test_store(8).await;
            
            let point = VectorPoint::new(id, vector_values);
            store.upsert(point).await.unwrap();
            
            // Verify it exists
            prop_assert!(store.exists(id).await.unwrap());
            
            // Delete it
            let deleted = store.delete(id).await.unwrap();
            prop_assert!(deleted);
            
            // Verify it's gone
            prop_assert!(!store.exists(id).await.unwrap());
            prop_assert!(store.get(id).await.unwrap().is_none());
        });
    }
}
