//! VectorStore implementation using Qdrant embedded mode
//!
//! Provides vector storage and retrieval for semantic search functionality.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::config::{Distance, VectorStoreConfig};
use super::error::VectorError;
use super::payload_fields;

/// Result type for vector operations
pub type VectorResult<T> = Result<T, VectorError>;

/// A point in the vector space with associated payload
#[derive(Debug, Clone)]
pub struct VectorPoint {
    /// Unique point ID
    pub id: u64,
    /// Vector embedding
    pub vector: Vec<f32>,
    /// Payload data for filtering
    pub payload: HashMap<String, Value>,
}

impl VectorPoint {
    /// Create a new vector point
    pub fn new(id: u64, vector: Vec<f32>) -> Self {
        Self {
            id,
            vector,
            payload: HashMap::new(),
        }
    }

    /// Add a payload field
    pub fn with_payload(mut self, key: impl Into<String>, value: Value) -> Self {
        self.payload.insert(key.into(), value);
        self
    }

    /// Add file_id to payload
    pub fn with_file_id(self, file_id: Uuid) -> Self {
        self.with_payload(payload_fields::FILE_ID, Value::String(file_id.to_string()))
    }

    /// Add chunk_id to payload
    pub fn with_chunk_id(self, chunk_id: Uuid) -> Self {
        self.with_payload(payload_fields::CHUNK_ID, Value::String(chunk_id.to_string()))
    }

    /// Add file_type to payload
    pub fn with_file_type(self, file_type: &str) -> Self {
        self.with_payload(payload_fields::FILE_TYPE, Value::String(file_type.to_string()))
    }

    /// Add tag_ids to payload
    pub fn with_tag_ids(self, tag_ids: Vec<Uuid>) -> Self {
        let tags: Vec<Value> = tag_ids.iter().map(|id| Value::String(id.to_string())).collect();
        self.with_payload(payload_fields::TAG_IDS, Value::Array(tags))
    }

    /// Add privacy_level to payload
    pub fn with_privacy_level(self, level: &str) -> Self {
        self.with_payload(payload_fields::PRIVACY_LEVEL, Value::String(level.to_string()))
    }
}

/// Search result from vector query
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Point ID
    pub id: u64,
    /// Similarity score (0.0 - 1.0 for cosine)
    pub score: f32,
    /// Payload data
    pub payload: HashMap<String, Value>,
    /// The vector (optional, only if requested)
    pub vector: Option<Vec<f32>>,
}

impl SearchResult {
    /// Extract file_id from payload
    pub fn file_id(&self) -> Option<Uuid> {
        self.payload
            .get(payload_fields::FILE_ID)
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
    }

    /// Extract chunk_id from payload
    pub fn chunk_id(&self) -> Option<Uuid> {
        self.payload
            .get(payload_fields::CHUNK_ID)
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
    }
}

/// Filter conditions for vector search
#[derive(Debug, Clone, Default)]
pub struct SearchFilter {
    /// Filter by file types (OR logic)
    pub file_types: Option<Vec<String>>,
    /// Filter by tag IDs (AND logic)
    pub tag_ids: Option<Vec<Uuid>>,
    /// Exclude private files
    pub exclude_private: bool,
    /// Filter by file IDs (OR logic)
    pub file_ids: Option<Vec<Uuid>>,
}

impl SearchFilter {
    /// Create a new empty filter
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by file types
    pub fn with_file_types(mut self, types: Vec<String>) -> Self {
        self.file_types = Some(types);
        self
    }

    /// Filter by tag IDs
    pub fn with_tag_ids(mut self, ids: Vec<Uuid>) -> Self {
        self.tag_ids = Some(ids);
        self
    }

    /// Exclude private files
    pub fn exclude_private(mut self) -> Self {
        self.exclude_private = true;
        self
    }

    /// Filter by specific file IDs
    pub fn with_file_ids(mut self, ids: Vec<Uuid>) -> Self {
        self.file_ids = Some(ids);
        self
    }
}


/// Vector store using Qdrant embedded mode
/// 
/// This provides semantic search capabilities by storing and querying
/// vector embeddings of file content.
pub struct VectorStore {
    /// Configuration
    config: VectorStoreConfig,
    /// Storage path
    storage_path: PathBuf,
    /// In-memory vector storage (stub implementation)
    /// In production, this would be replaced with actual Qdrant client
    vectors: Arc<RwLock<HashMap<u64, StoredVector>>>,
    /// Next available point ID
    next_id: Arc<RwLock<u64>>,
    /// Whether the store is initialized
    initialized: Arc<RwLock<bool>>,
}

/// Internal storage for vectors (stub implementation)
#[derive(Debug, Clone)]
struct StoredVector {
    vector: Vec<f32>,
    payload: HashMap<String, Value>,
}

impl VectorStore {
    /// Create a new VectorStore with the given configuration
    /// 
    /// This will:
    /// 1. Clean up any residual lock files from previous runs
    /// 2. Initialize the storage directory
    /// 3. Create the collection if it doesn't exist
    pub async fn new(config: VectorStoreConfig) -> VectorResult<Self> {
        let storage_path = PathBuf::from(&config.storage_path);
        
        info!(
            "Initializing VectorStore at {:?} with collection '{}'",
            storage_path, config.collection_name
        );

        // Ensure storage directory exists
        if !storage_path.exists() {
            std::fs::create_dir_all(&storage_path).map_err(|e| {
                VectorError::StoragePathError {
                    reason: format!("Failed to create storage directory: {}", e),
                }
            })?;
            debug!("Created storage directory: {:?}", storage_path);
        }

        // Clean up lock files if configured
        if config.cleanup_locks_on_startup {
            Self::cleanup_lock_files(&storage_path)?;
        }

        let store = Self {
            config,
            storage_path,
            vectors: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(RwLock::new(1)),
            initialized: Arc::new(RwLock::new(false)),
        };

        // Initialize the collection
        store.ensure_collection().await?;

        *store.initialized.write().await = true;
        info!("VectorStore initialized successfully");

        Ok(store)
    }

    /// Clean up residual lock files from previous runs
    /// 
    /// Qdrant can leave .lock files if it crashes or is killed unexpectedly.
    /// These need to be cleaned up before starting a new instance.
    fn cleanup_lock_files(storage_path: &Path) -> VectorResult<()> {
        if !storage_path.exists() {
            return Ok(());
        }

        let lock_patterns = [".lock", ".wal.lock", "storage.lock"];
        
        fn find_and_remove_locks(dir: &Path, patterns: &[&str]) -> VectorResult<u32> {
            let mut removed = 0;
            
            let entries = match std::fs::read_dir(dir) {
                Ok(e) => e,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
                Err(e) => return Err(VectorError::Io(e)),
            };

            for entry in entries.flatten() {
                let path = entry.path();
                
                if path.is_dir() {
                    removed += find_and_remove_locks(&path, patterns)?;
                } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    for pattern in patterns {
                        if name.ends_with(pattern) || name == *pattern {
                            match std::fs::remove_file(&path) {
                                Ok(_) => {
                                    warn!("Removed stale lock file: {:?}", path);
                                    removed += 1;
                                }
                                Err(e) => {
                                    error!("Failed to remove lock file {:?}: {}", path, e);
                                    return Err(VectorError::LockFileCleanupFailed {
                                        path: path.display().to_string(),
                                    });
                                }
                            }
                            break;
                        }
                    }
                }
            }
            
            Ok(removed)
        }

        let removed = find_and_remove_locks(storage_path, &lock_patterns)?;
        if removed > 0 {
            info!("Cleaned up {} stale lock file(s)", removed);
        }

        Ok(())
    }

    /// Ensure the collection exists, creating it if necessary
    async fn ensure_collection(&self) -> VectorResult<()> {
        // In a real implementation, this would use the Qdrant client to:
        // 1. Check if collection exists
        // 2. Create it with proper HNSW config if not
        // 
        // For now, this is a stub that just logs the operation
        debug!(
            "Ensuring collection '{}' exists with vector size {}",
            self.config.collection_name, self.config.vector_size
        );

        // The stub implementation uses in-memory storage, so no actual
        // collection creation is needed
        Ok(())
    }

    /// Get the configuration
    pub fn config(&self) -> &VectorStoreConfig {
        &self.config
    }

    /// Get the storage path
    pub fn storage_path(&self) -> &Path {
        &self.storage_path
    }

    /// Check if the store is initialized
    pub async fn is_initialized(&self) -> bool {
        *self.initialized.read().await
    }

    /// Get the number of vectors in the store
    pub async fn count(&self) -> VectorResult<u64> {
        let vectors = self.vectors.read().await;
        Ok(vectors.len() as u64)
    }

    /// Generate a new unique point ID
    async fn generate_id(&self) -> u64 {
        let mut next_id = self.next_id.write().await;
        let id = *next_id;
        *next_id += 1;
        id
    }
}


// ============================================================================
// CRUD Operations
// ============================================================================

impl VectorStore {
    /// Insert or update a single vector point
    /// 
    /// If a point with the same ID exists, it will be replaced.
    pub async fn upsert(&self, point: VectorPoint) -> VectorResult<u64> {
        self.validate_vector_dimension(&point.vector)?;

        let mut vectors = self.vectors.write().await;
        let id = point.id;
        
        vectors.insert(
            id,
            StoredVector {
                vector: point.vector,
                payload: point.payload,
            },
        );

        debug!("Upserted vector point with ID {}", id);
        Ok(id)
    }

    /// Insert or update multiple vector points in batch
    /// 
    /// This is more efficient than calling upsert() multiple times.
    pub async fn upsert_batch(&self, points: Vec<VectorPoint>) -> VectorResult<Vec<u64>> {
        if points.is_empty() {
            return Ok(vec![]);
        }

        // Validate all vectors first
        for point in &points {
            self.validate_vector_dimension(&point.vector)?;
        }

        let mut vectors = self.vectors.write().await;
        let mut ids = Vec::with_capacity(points.len());

        for point in points {
            let id = point.id;
            vectors.insert(
                id,
                StoredVector {
                    vector: point.vector,
                    payload: point.payload,
                },
            );
            ids.push(id);
        }

        debug!("Batch upserted {} vector points", ids.len());
        Ok(ids)
    }

    /// Insert a new vector and return its generated ID
    pub async fn insert(&self, vector: Vec<f32>, payload: HashMap<String, Value>) -> VectorResult<u64> {
        self.validate_vector_dimension(&vector)?;

        let id = self.generate_id().await;
        let point = VectorPoint {
            id,
            vector,
            payload,
        };

        self.upsert(point).await
    }

    /// Search for similar vectors
    /// 
    /// Returns the top `limit` most similar vectors to the query vector.
    pub async fn search(
        &self,
        query_vector: &[f32],
        limit: usize,
        filter: Option<SearchFilter>,
    ) -> VectorResult<Vec<SearchResult>> {
        self.validate_vector_dimension(query_vector)?;

        let vectors = self.vectors.read().await;
        
        // Calculate similarity scores for all vectors
        let mut scored: Vec<(u64, f32, &StoredVector)> = vectors
            .iter()
            .filter(|(_, stored)| self.matches_filter(stored, filter.as_ref()))
            .map(|(id, stored)| {
                let score = self.calculate_similarity(query_vector, &stored.vector);
                (*id, score, stored)
            })
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top `limit` results
        let results: Vec<SearchResult> = scored
            .into_iter()
            .take(limit)
            .map(|(id, score, stored)| SearchResult {
                id,
                score,
                payload: stored.payload.clone(),
                vector: None,
            })
            .collect();

        debug!(
            "Search returned {} results (limit: {})",
            results.len(),
            limit
        );

        Ok(results)
    }

    /// Search with vector retrieval
    /// 
    /// Same as search() but also returns the vectors themselves.
    pub async fn search_with_vectors(
        &self,
        query_vector: &[f32],
        limit: usize,
        filter: Option<SearchFilter>,
    ) -> VectorResult<Vec<SearchResult>> {
        self.validate_vector_dimension(query_vector)?;

        let vectors = self.vectors.read().await;
        
        let mut scored: Vec<(u64, f32, &StoredVector)> = vectors
            .iter()
            .filter(|(_, stored)| self.matches_filter(stored, filter.as_ref()))
            .map(|(id, stored)| {
                let score = self.calculate_similarity(query_vector, &stored.vector);
                (*id, score, stored)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let results: Vec<SearchResult> = scored
            .into_iter()
            .take(limit)
            .map(|(id, score, stored)| SearchResult {
                id,
                score,
                payload: stored.payload.clone(),
                vector: Some(stored.vector.clone()),
            })
            .collect();

        Ok(results)
    }

    /// Delete a vector by ID
    pub async fn delete(&self, id: u64) -> VectorResult<bool> {
        let mut vectors = self.vectors.write().await;
        let removed = vectors.remove(&id).is_some();
        
        if removed {
            debug!("Deleted vector point with ID {}", id);
        }
        
        Ok(removed)
    }

    /// Delete multiple vectors by ID
    pub async fn delete_batch(&self, ids: &[u64]) -> VectorResult<u64> {
        let mut vectors = self.vectors.write().await;
        let mut deleted = 0u64;

        for id in ids {
            if vectors.remove(id).is_some() {
                deleted += 1;
            }
        }

        debug!("Batch deleted {} vector points", deleted);
        Ok(deleted)
    }

    /// Delete vectors by file ID
    /// 
    /// Removes all vectors associated with a specific file.
    pub async fn delete_by_file_id(&self, file_id: Uuid) -> VectorResult<u64> {
        let file_id_str = file_id.to_string();
        let mut vectors = self.vectors.write().await;
        
        let ids_to_remove: Vec<u64> = vectors
            .iter()
            .filter(|(_, stored)| {
                stored
                    .payload
                    .get(payload_fields::FILE_ID)
                    .and_then(|v| v.as_str())
                    .map(|s| s == file_id_str)
                    .unwrap_or(false)
            })
            .map(|(id, _)| *id)
            .collect();

        let deleted = ids_to_remove.len() as u64;
        for id in ids_to_remove {
            vectors.remove(&id);
        }

        debug!(
            "Deleted {} vector points for file_id {}",
            deleted, file_id
        );
        Ok(deleted)
    }

    /// Get a vector by ID
    pub async fn get(&self, id: u64) -> VectorResult<Option<SearchResult>> {
        let vectors = self.vectors.read().await;
        
        Ok(vectors.get(&id).map(|stored| SearchResult {
            id,
            score: 1.0, // Perfect match for direct retrieval
            payload: stored.payload.clone(),
            vector: Some(stored.vector.clone()),
        }))
    }

    /// Get multiple vectors by ID
    pub async fn get_batch(&self, ids: &[u64]) -> VectorResult<Vec<SearchResult>> {
        let vectors = self.vectors.read().await;
        
        let results: Vec<SearchResult> = ids
            .iter()
            .filter_map(|id| {
                vectors.get(id).map(|stored| SearchResult {
                    id: *id,
                    score: 1.0,
                    payload: stored.payload.clone(),
                    vector: Some(stored.vector.clone()),
                })
            })
            .collect();

        Ok(results)
    }

    /// Check if a vector exists
    pub async fn exists(&self, id: u64) -> VectorResult<bool> {
        let vectors = self.vectors.read().await;
        Ok(vectors.contains_key(&id))
    }

    /// Clear all vectors from the store
    pub async fn clear(&self) -> VectorResult<u64> {
        let mut vectors = self.vectors.write().await;
        let count = vectors.len() as u64;
        vectors.clear();
        
        // Reset ID counter
        *self.next_id.write().await = 1;
        
        info!("Cleared {} vectors from store", count);
        Ok(count)
    }
}


// ============================================================================
// Helper Methods
// ============================================================================

impl VectorStore {
    /// Validate that a vector has the correct dimension
    fn validate_vector_dimension(&self, vector: &[f32]) -> VectorResult<()> {
        let expected = self.config.vector_size as usize;
        let actual = vector.len();
        
        if actual != expected {
            return Err(VectorError::InvalidDimension {
                expected: expected as u64,
                actual: actual as u64,
            });
        }
        
        Ok(())
    }

    /// Calculate similarity between two vectors based on configured distance metric
    fn calculate_similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        match self.config.distance {
            Distance::Cosine => self.cosine_similarity(a, b),
            Distance::Euclidean => self.euclidean_similarity(a, b),
            Distance::Dot => self.dot_product(a, b),
        }
    }

    /// Calculate cosine similarity between two vectors
    /// Returns a value between -1 and 1, where 1 means identical direction
    fn cosine_similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        
        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }
        
        dot / (norm_a * norm_b)
    }

    /// Calculate Euclidean similarity (inverse of distance)
    /// Returns a value between 0 and 1, where 1 means identical
    fn euclidean_similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        let distance: f32 = a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y).powi(2))
            .sum::<f32>()
            .sqrt();
        
        // Convert distance to similarity (1 / (1 + distance))
        1.0 / (1.0 + distance)
    }

    /// Calculate dot product between two vectors
    fn dot_product(&self, a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
    }

    /// Check if a stored vector matches the filter conditions
    fn matches_filter(&self, stored: &StoredVector, filter: Option<&SearchFilter>) -> bool {
        let filter = match filter {
            Some(f) => f,
            None => return true,
        };

        // Check file type filter
        if let Some(ref file_types) = filter.file_types {
            let stored_type = stored
                .payload
                .get(payload_fields::FILE_TYPE)
                .and_then(|v| v.as_str());
            
            match stored_type {
                Some(t) => {
                    if !file_types.iter().any(|ft| ft == t) {
                        return false;
                    }
                }
                None => return false,
            }
        }

        // Check privacy level filter
        if filter.exclude_private {
            let privacy = stored
                .payload
                .get(payload_fields::PRIVACY_LEVEL)
                .and_then(|v| v.as_str());
            
            if privacy == Some("Private") {
                return false;
            }
        }

        // Check file ID filter
        if let Some(ref file_ids) = filter.file_ids {
            let stored_file_id = stored
                .payload
                .get(payload_fields::FILE_ID)
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok());
            
            match stored_file_id {
                Some(id) => {
                    if !file_ids.contains(&id) {
                        return false;
                    }
                }
                None => return false,
            }
        }

        // Check tag ID filter (AND logic - must have all tags)
        if let Some(ref tag_ids) = filter.tag_ids {
            let stored_tags: Vec<Uuid> = stored
                .payload
                .get(payload_fields::TAG_IDS)
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .filter_map(|s| Uuid::parse_str(s).ok())
                        .collect()
                })
                .unwrap_or_default();
            
            for required_tag in tag_ids {
                if !stored_tags.contains(required_tag) {
                    return false;
                }
            }
        }

        true
    }
}

// ============================================================================
// Serialization Support for Property Testing
// ============================================================================

#[cfg(test)]
impl VectorPoint {
    /// Create from components for testing
    pub fn from_parts(id: u64, vector: Vec<f32>, payload: HashMap<String, Value>) -> Self {
        Self { id, vector, payload }
    }
}
