//! Hybrid Inference Engine for NeuralFS
//!
//! This module provides parallel local and cloud inference:
//! - Simultaneous dispatch to local and cloud engines
//! - Timeout handling for cloud requests
//! - Result caching for performance
//! - Graceful degradation when cloud is unavailable
//!
//! **Validates: Requirements 11.1**

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;
use uuid::Uuid;

use crate::embeddings::EmbeddingEngine;

use super::cloud::{CloudBridge, CloudConfig, CloudInferenceResult};
use super::error::{InferenceError, InferenceResult};
use super::local::{LocalInferenceEngine, LocalInferenceResult};
use super::merger::{MergerConfig, ResultMerger};
use super::types::{InferenceRequest, InferenceResponse, InferenceSource};

/// Hybrid inference engine that coordinates local and cloud inference
pub struct HybridInferenceEngine {
    /// Local inference engine
    local_engine: LocalInferenceEngine,
    
    /// Cloud inference bridge
    cloud_bridge: Option<CloudBridge>,
    
    /// Result merger
    result_merger: ResultMerger,
    
    /// Inference cache
    cache: InferenceCache,
    
    /// Whether cloud is enabled
    cloud_enabled: bool,
}

impl HybridInferenceEngine {
    /// Create a new hybrid inference engine
    pub fn new(embedding_engine: Arc<EmbeddingEngine>) -> Self {
        Self {
            local_engine: LocalInferenceEngine::new(embedding_engine),
            cloud_bridge: None,
            result_merger: ResultMerger::new(),
            cache: InferenceCache::new(1000),
            cloud_enabled: false,
        }
    }
    
    /// Create a hybrid inference engine with cloud support
    pub fn with_cloud(embedding_engine: Arc<EmbeddingEngine>, cloud_config: CloudConfig) -> Self {
        let cloud_enabled = cloud_config.enabled;
        Self {
            local_engine: LocalInferenceEngine::new(embedding_engine),
            cloud_bridge: Some(CloudBridge::new(cloud_config)),
            result_merger: ResultMerger::new(),
            cache: InferenceCache::new(1000),
            cloud_enabled,
        }
    }
    
    /// Configure the result merger
    pub fn with_merger_config(mut self, config: MergerConfig) -> Self {
        self.result_merger = ResultMerger::with_config(config);
        self
    }
    
    /// Configure the cache size
    pub fn with_cache_size(mut self, max_entries: usize) -> Self {
        self.cache = InferenceCache::new(max_entries);
        self
    }
    
    /// Perform hybrid inference on a request
    ///
    /// This method:
    /// 1. Checks the cache for existing results
    /// 2. Dispatches to local and cloud engines in parallel
    /// 3. Waits for local results immediately
    /// 4. Waits for cloud results with timeout
    /// 5. Merges results and caches them
    pub async fn infer(&self, request: InferenceRequest) -> InferenceResult<InferenceResponse> {
        let request_id = request.request_id;
        let start = Instant::now();
        
        // 1. Check cache if enabled
        if request.options.use_cache {
            if let Some(cached) = self.cache.get(&request.query).await {
                tracing::debug!("Cache hit for query: {}", request.query);
                return Ok(cached);
            }
        }
        
        // 2. Determine if cloud should be used
        let use_cloud = request.options.enable_cloud 
            && self.cloud_enabled 
            && self.cloud_bridge.is_some()
            && self.cloud_bridge.as_ref().map(|b| b.is_available()).unwrap_or(false);
        
        // 3. Dispatch to local and cloud in parallel
        let local_future = self.local_engine.infer(&request);
        
        let cloud_future = if use_cloud {
            let cloud_bridge = self.cloud_bridge.as_ref().unwrap();
            let timeout_ms = request.options.cloud_timeout_ms;
            
            // Create a future that will be awaited with timeout
            Some(async move {
                // First get local result to generate cloud prompt
                // Note: In a real implementation, we'd want to avoid this dependency
                // For now, we'll use a simple prompt
                let prompt = format!(
                    "用户查询: \"{}\"\n\n请分析用户意图并提供搜索建议。",
                    request.query
                );
                
                tokio::time::timeout(
                    Duration::from_millis(timeout_ms),
                    cloud_bridge.infer(&request, &prompt)
                ).await
            })
        } else {
            None
        };
        
        // 4. Wait for local result (always)
        let local_result = local_future.await?;
        
        // 5. Wait for cloud result (with timeout)
        let cloud_result: Option<CloudInferenceResult> = if let Some(cloud_fut) = cloud_future {
            match cloud_fut.await {
                Ok(Ok(result)) => {
                    tracing::debug!("Cloud inference succeeded");
                    Some(result)
                }
                Ok(Err(e)) => {
                    tracing::warn!("Cloud inference failed: {}", e);
                    None
                }
                Err(_) => {
                    tracing::warn!("Cloud inference timed out");
                    None
                }
            }
        } else {
            None
        };
        
        // 6. Merge results
        let response = self.result_merger.merge(request_id, local_result, cloud_result);
        
        // 7. Cache the result
        if request.options.use_cache {
            self.cache.put(&request.query, response.clone()).await;
        }
        
        let total_duration = start.elapsed().as_millis() as u64;
        
        // Update duration in response
        let mut final_response = response;
        final_response.duration_ms = total_duration;
        
        Ok(final_response)
    }
    
    /// Perform local-only inference (no cloud)
    pub async fn infer_local(&self, request: &InferenceRequest) -> InferenceResult<LocalInferenceResult> {
        self.local_engine.infer(request).await
    }
    
    /// Perform cloud-only inference (requires local prompt generation first)
    pub async fn infer_cloud(&self, request: &InferenceRequest, prompt: &str) -> InferenceResult<CloudInferenceResult> {
        let cloud_bridge = self.cloud_bridge.as_ref()
            .ok_or(InferenceError::CloudUnavailable {
                reason: "Cloud bridge not configured".to_string(),
            })?;
        
        cloud_bridge.infer(request, prompt).await
    }
    
    /// Check if cloud inference is available
    pub fn is_cloud_available(&self) -> bool {
        self.cloud_enabled 
            && self.cloud_bridge.as_ref().map(|b| b.is_available()).unwrap_or(false)
    }
    
    /// Enable or disable cloud inference
    pub fn set_cloud_enabled(&mut self, enabled: bool) {
        self.cloud_enabled = enabled;
    }
    
    /// Get cache statistics
    pub async fn cache_stats(&self) -> CacheStats {
        self.cache.stats().await
    }
    
    /// Clear the inference cache
    pub async fn clear_cache(&self) {
        self.cache.clear().await;
    }
    
    /// Get reference to local engine
    pub fn local_engine(&self) -> &LocalInferenceEngine {
        &self.local_engine
    }
    
    /// Get reference to cloud bridge
    pub fn cloud_bridge(&self) -> Option<&CloudBridge> {
        self.cloud_bridge.as_ref()
    }
    
    /// Get reference to result merger
    pub fn result_merger(&self) -> &ResultMerger {
        &self.result_merger
    }
}

/// Inference cache for storing recent results
pub struct InferenceCache {
    /// Cache entries
    entries: RwLock<HashMap<String, CacheEntry>>,
    
    /// Maximum number of entries
    max_entries: usize,
    
    /// Cache statistics
    stats: RwLock<CacheStatsInternal>,
}

/// Cache entry
struct CacheEntry {
    /// Cached response
    response: InferenceResponse,
    
    /// Entry creation time
    created_at: Instant,
    
    /// Last access time
    last_accessed: Instant,
    
    /// Access count
    access_count: u64,
}

/// Internal cache statistics
struct CacheStatsInternal {
    hits: u64,
    misses: u64,
    evictions: u64,
}

/// Public cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Number of cache hits
    pub hits: u64,
    
    /// Number of cache misses
    pub misses: u64,
    
    /// Number of evictions
    pub evictions: u64,
    
    /// Current entry count
    pub entry_count: usize,
    
    /// Maximum entries
    pub max_entries: usize,
    
    /// Hit rate (0.0 - 1.0)
    pub hit_rate: f64,
}

impl InferenceCache {
    /// Create a new inference cache
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            max_entries,
            stats: RwLock::new(CacheStatsInternal {
                hits: 0,
                misses: 0,
                evictions: 0,
            }),
        }
    }
    
    /// Get a cached response
    pub async fn get(&self, query: &str) -> Option<InferenceResponse> {
        let cache_key = self.normalize_key(query);
        
        // Try to get from cache
        let mut entries = self.entries.write().await;
        
        if let Some(entry) = entries.get_mut(&cache_key) {
            // Check if entry is still valid (TTL: 5 minutes)
            if entry.created_at.elapsed() < Duration::from_secs(300) {
                entry.last_accessed = Instant::now();
                entry.access_count += 1;
                
                // Update stats
                let mut stats = self.stats.write().await;
                stats.hits += 1;
                
                // Clone and update response to indicate cache hit
                let mut response = entry.response.clone();
                if !response.sources.contains(&InferenceSource::Cache) {
                    response.sources.push(InferenceSource::Cache);
                }
                
                return Some(response);
            } else {
                // Entry expired, remove it
                entries.remove(&cache_key);
            }
        }
        
        // Update miss stats
        let mut stats = self.stats.write().await;
        stats.misses += 1;
        
        None
    }
    
    /// Put a response in the cache
    pub async fn put(&self, query: &str, response: InferenceResponse) {
        let cache_key = self.normalize_key(query);
        
        let mut entries = self.entries.write().await;
        
        // Evict if necessary
        if entries.len() >= self.max_entries && !entries.contains_key(&cache_key) {
            self.evict_lru(&mut entries).await;
        }
        
        // Insert new entry
        entries.insert(cache_key, CacheEntry {
            response,
            created_at: Instant::now(),
            last_accessed: Instant::now(),
            access_count: 1,
        });
    }
    
    /// Evict least recently used entry
    async fn evict_lru(&self, entries: &mut HashMap<String, CacheEntry>) {
        if let Some((key, _)) = entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_accessed)
            .map(|(k, v)| (k.clone(), v.last_accessed))
        {
            entries.remove(&key);
            
            let mut stats = self.stats.write().await;
            stats.evictions += 1;
        }
    }
    
    /// Clear all cache entries
    pub async fn clear(&self) {
        let mut entries = self.entries.write().await;
        entries.clear();
    }
    
    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        let entries = self.entries.read().await;
        let stats = self.stats.read().await;
        
        let total = stats.hits + stats.misses;
        let hit_rate = if total > 0 {
            stats.hits as f64 / total as f64
        } else {
            0.0
        };
        
        CacheStats {
            hits: stats.hits,
            misses: stats.misses,
            evictions: stats.evictions,
            entry_count: entries.len(),
            max_entries: self.max_entries,
            hit_rate,
        }
    }
    
    /// Normalize cache key (lowercase, trim whitespace)
    fn normalize_key(&self, query: &str) -> String {
        query.trim().to_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embeddings::EmbeddingConfig;
    use crate::inference::types::InferenceContext;

    fn create_test_engine() -> HybridInferenceEngine {
        let embedding_engine = Arc::new(EmbeddingEngine::new(EmbeddingConfig::default()));
        HybridInferenceEngine::new(embedding_engine)
    }

    #[tokio::test]
    async fn test_cache_put_and_get() {
        let cache = InferenceCache::new(10);
        
        let response = InferenceResponse {
            request_id: Uuid::now_v7(),
            intent: crate::core::types::search::SearchIntent::FindFile {
                file_type_hint: None,
                time_hint: None,
            },
            query_embedding: vec![0.1, 0.2, 0.3],
            cloud_understanding: None,
            cloud_enhanced: false,
            duration_ms: 100,
            sources: vec![InferenceSource::LocalEmbedding],
        };
        
        cache.put("test query", response.clone()).await;
        
        let cached = cache.get("test query").await;
        assert!(cached.is_some());
        
        let cached_response = cached.unwrap();
        assert_eq!(cached_response.request_id, response.request_id);
        assert!(cached_response.sources.contains(&InferenceSource::Cache));
    }

    #[tokio::test]
    async fn test_cache_miss() {
        let cache = InferenceCache::new(10);
        
        let cached = cache.get("nonexistent query").await;
        assert!(cached.is_none());
        
        let stats = cache.stats().await;
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hits, 0);
    }

    #[tokio::test]
    async fn test_cache_eviction() {
        let cache = InferenceCache::new(2);
        
        let response = InferenceResponse {
            request_id: Uuid::now_v7(),
            intent: crate::core::types::search::SearchIntent::FindFile {
                file_type_hint: None,
                time_hint: None,
            },
            query_embedding: vec![],
            cloud_understanding: None,
            cloud_enhanced: false,
            duration_ms: 0,
            sources: vec![],
        };
        
        cache.put("query1", response.clone()).await;
        cache.put("query2", response.clone()).await;
        cache.put("query3", response.clone()).await; // Should evict query1
        
        let stats = cache.stats().await;
        assert_eq!(stats.entry_count, 2);
        assert_eq!(stats.evictions, 1);
    }

    #[tokio::test]
    async fn test_cache_clear() {
        let cache = InferenceCache::new(10);
        
        let response = InferenceResponse {
            request_id: Uuid::now_v7(),
            intent: crate::core::types::search::SearchIntent::FindFile {
                file_type_hint: None,
                time_hint: None,
            },
            query_embedding: vec![],
            cloud_understanding: None,
            cloud_enhanced: false,
            duration_ms: 0,
            sources: vec![],
        };
        
        cache.put("query1", response.clone()).await;
        cache.put("query2", response.clone()).await;
        
        cache.clear().await;
        
        let stats = cache.stats().await;
        assert_eq!(stats.entry_count, 0);
    }

    #[tokio::test]
    async fn test_cache_key_normalization() {
        let cache = InferenceCache::new(10);
        
        let response = InferenceResponse {
            request_id: Uuid::now_v7(),
            intent: crate::core::types::search::SearchIntent::FindFile {
                file_type_hint: None,
                time_hint: None,
            },
            query_embedding: vec![],
            cloud_understanding: None,
            cloud_enhanced: false,
            duration_ms: 0,
            sources: vec![],
        };
        
        cache.put("Test Query", response.clone()).await;
        
        // Should find with different case
        let cached = cache.get("test query").await;
        assert!(cached.is_some());
        
        // Should find with extra whitespace
        let cached = cache.get("  test query  ").await;
        assert!(cached.is_some());
    }

    #[test]
    fn test_hybrid_engine_creation() {
        let engine = create_test_engine();
        assert!(!engine.is_cloud_available());
    }

    #[test]
    fn test_hybrid_engine_with_cloud() {
        let embedding_engine = Arc::new(EmbeddingEngine::new(EmbeddingConfig::default()));
        let cloud_config = CloudConfig::default();
        
        let engine = HybridInferenceEngine::with_cloud(embedding_engine, cloud_config);
        // Cloud is disabled by default in CloudConfig::default()
        assert!(!engine.is_cloud_available());
    }

    #[test]
    fn test_set_cloud_enabled() {
        let mut engine = create_test_engine();
        
        engine.set_cloud_enabled(true);
        // Still not available because no cloud bridge
        assert!(!engine.is_cloud_available());
    }
}
