//! Vector store configuration

use serde::{Deserialize, Serialize};

/// Distance metric for vector similarity
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Distance {
    /// Cosine similarity (normalized dot product)
    Cosine,
    /// Euclidean distance (L2)
    Euclidean,
    /// Dot product (inner product)
    Dot,
}

impl Default for Distance {
    fn default() -> Self {
        Distance::Cosine
    }
}

/// HNSW index configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnswConfig {
    /// Number of edges per node in the index graph
    /// Higher values = better recall, more memory
    pub m: u64,
    
    /// Number of neighbors to consider during index construction
    /// Higher values = better quality, slower build
    pub ef_construct: u64,
    
    /// Threshold below which full scan is used instead of HNSW
    pub full_scan_threshold: u64,
}

impl Default for HnswConfig {
    fn default() -> Self {
        Self {
            m: 16,
            ef_construct: 100,
            full_scan_threshold: 10000,
        }
    }
}

/// Optimizer configuration for the vector store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizerConfig {
    /// Threshold ratio of deleted vectors to trigger vacuum
    pub deleted_threshold: f64,
    
    /// Minimum number of vectors before vacuum is considered
    pub vacuum_min_vector_number: u64,
    
    /// Default number of segments
    pub default_segment_number: u64,
    
    /// Maximum vectors per segment
    pub max_segment_size: u64,
    
    /// Threshold to switch to memory-mapped storage
    pub memmap_threshold: u64,
    
    /// Threshold to start building HNSW index
    pub indexing_threshold: u64,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            deleted_threshold: 0.2,
            vacuum_min_vector_number: 1000,
            default_segment_number: 4,
            max_segment_size: 200000,
            memmap_threshold: 50000,
            indexing_threshold: 20000,
        }
    }
}

/// Main configuration for the vector store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorStoreConfig {
    /// Collection name in Qdrant
    pub collection_name: String,
    
    /// Vector dimension (must match embedding model output)
    pub vector_size: u64,
    
    /// Distance metric for similarity
    pub distance: Distance,
    
    /// HNSW index configuration
    pub hnsw_config: HnswConfig,
    
    /// Optimizer configuration
    pub optimizer_config: OptimizerConfig,
    
    /// Storage path for Qdrant data
    pub storage_path: String,
    
    /// Whether to clean up lock files on startup
    pub cleanup_locks_on_startup: bool,
}

impl Default for VectorStoreConfig {
    fn default() -> Self {
        Self {
            collection_name: "neuralfs_vectors".to_string(),
            vector_size: 384, // all-MiniLM-L6-v2 output dimension
            distance: Distance::default(),
            hnsw_config: HnswConfig::default(),
            optimizer_config: OptimizerConfig::default(),
            storage_path: "data/qdrant".to_string(),
            cleanup_locks_on_startup: true,
        }
    }
}

impl VectorStoreConfig {
    /// Create a new config with custom collection name
    pub fn with_collection_name(mut self, name: impl Into<String>) -> Self {
        self.collection_name = name.into();
        self
    }

    /// Create a new config with custom vector size
    pub fn with_vector_size(mut self, size: u64) -> Self {
        self.vector_size = size;
        self
    }

    /// Create a new config with custom storage path
    pub fn with_storage_path(mut self, path: impl Into<String>) -> Self {
        self.storage_path = path.into();
        self
    }

    /// Create a new config with custom distance metric
    pub fn with_distance(mut self, distance: Distance) -> Self {
        self.distance = distance;
        self
    }
}
