//! VRAM Manager for tracking and managing GPU memory usage
//!
//! Implements:
//! - VRAM usage tracking
//! - LRU model eviction
//! - Memory limit enforcement (default 4GB)

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tokio::sync::RwLock;
use serde::Serialize;

use super::model_manager::ModelId;

/// Information about a loaded model
#[derive(Debug, Clone)]
pub struct ModelInfo {
    /// Model ID
    pub id: ModelId,
    
    /// Model name for display
    pub name: String,
    
    /// VRAM usage in bytes
    pub vram_bytes: u64,
    
    /// Last time this model was used
    pub last_used: Instant,
    
    /// Number of times this model has been used
    pub use_count: u64,
}

/// Current VRAM status
#[derive(Debug, Clone, Serialize)]
pub struct VRAMStatus {
    /// Currently used VRAM in bytes
    pub used_bytes: u64,
    
    /// Maximum allowed VRAM in bytes
    pub max_bytes: u64,
    
    /// Number of loaded models
    pub loaded_models: usize,
    
    /// Usage percentage (0.0 - 1.0)
    pub usage_percent: f32,
}

/// VRAM Manager for tracking GPU memory usage
pub struct VRAMManager {
    /// Maximum VRAM usage in bytes
    max_vram_bytes: u64,
    
    /// Current VRAM usage in bytes (atomic for thread safety)
    current_usage: AtomicU64,
    
    /// Information about loaded models
    loaded_models: RwLock<HashMap<ModelId, ModelInfo>>,
}

impl VRAMManager {
    /// Create a new VRAM manager with the given limit in MB
    pub fn new(max_vram_mb: u64) -> Self {
        Self {
            max_vram_bytes: max_vram_mb * 1024 * 1024,
            current_usage: AtomicU64::new(0),
            loaded_models: RwLock::new(HashMap::new()),
        }
    }
    
    /// Get current VRAM status
    pub fn get_status(&self) -> VRAMStatus {
        let used_bytes = self.current_usage.load(Ordering::SeqCst);
        let max_bytes = self.max_vram_bytes;
        let loaded_models = {
            // Use try_read to avoid blocking, fall back to 0 if locked
            self.loaded_models
                .try_read()
                .map(|m| m.len())
                .unwrap_or(0)
        };
        
        VRAMStatus {
            used_bytes,
            max_bytes,
            loaded_models,
            usage_percent: if max_bytes > 0 {
                used_bytes as f32 / max_bytes as f32
            } else {
                0.0
            },
        }
    }
    
    /// Check if there's enough VRAM for a model
    pub fn has_available_vram(&self, needed_bytes: u64) -> bool {
        let current = self.current_usage.load(Ordering::SeqCst);
        current + needed_bytes <= self.max_vram_bytes
    }
    
    /// Get available VRAM in bytes
    pub fn available_vram(&self) -> u64 {
        let current = self.current_usage.load(Ordering::SeqCst);
        self.max_vram_bytes.saturating_sub(current)
    }
    
    /// Register a model as loaded
    pub async fn register_model(&self, model_info: ModelInfo) {
        let vram_bytes = model_info.vram_bytes;
        
        // Add to loaded models
        {
            let mut models = self.loaded_models.write().await;
            models.insert(model_info.id, model_info);
        }
        
        // Update usage counter
        self.current_usage.fetch_add(vram_bytes, Ordering::SeqCst);
        
        tracing::debug!(
            "Registered model, VRAM usage: {} MB / {} MB",
            self.current_usage.load(Ordering::SeqCst) / (1024 * 1024),
            self.max_vram_bytes / (1024 * 1024)
        );
    }
    
    /// Unregister a model (when unloaded)
    pub async fn unregister_model(&self, model_id: ModelId) {
        let vram_bytes = {
            let mut models = self.loaded_models.write().await;
            models.remove(&model_id).map(|m| m.vram_bytes).unwrap_or(0)
        };
        
        // Update usage counter
        self.current_usage.fetch_sub(vram_bytes, Ordering::SeqCst);
        
        tracing::debug!(
            "Unregistered model, VRAM usage: {} MB / {} MB",
            self.current_usage.load(Ordering::SeqCst) / (1024 * 1024),
            self.max_vram_bytes / (1024 * 1024)
        );
    }
    
    /// Record a model use (updates LRU tracking)
    pub async fn record_model_use(&self, model_id: ModelId) {
        let mut models = self.loaded_models.write().await;
        if let Some(model) = models.get_mut(&model_id) {
            model.last_used = Instant::now();
            model.use_count += 1;
        }
    }
    
    /// Evict models to free up the specified amount of VRAM
    /// Uses LRU (Least Recently Used) eviction strategy
    pub async fn evict_models(&self, needed_bytes: u64) -> Result<Vec<ModelId>, String> {
        let mut freed = 0u64;
        let mut to_evict = Vec::new();
        
        // Get models sorted by last used time (oldest first)
        let models = self.loaded_models.read().await;
        let mut sorted: Vec<_> = models.values().collect();
        sorted.sort_by_key(|m| m.last_used);
        
        // Select models to evict
        for model in sorted {
            if freed >= needed_bytes {
                break;
            }
            to_evict.push(model.id);
            freed += model.vram_bytes;
        }
        drop(models);
        
        if freed < needed_bytes {
            return Err(format!(
                "Cannot free enough VRAM: need {} MB, can free {} MB",
                needed_bytes / (1024 * 1024),
                freed / (1024 * 1024)
            ));
        }
        
        // Actually evict the models
        for model_id in &to_evict {
            self.unregister_model(*model_id).await;
        }
        
        tracing::info!(
            "Evicted {} models, freed {} MB",
            to_evict.len(),
            freed / (1024 * 1024)
        );
        
        Ok(to_evict)
    }
    
    /// Evict all models
    pub async fn evict_all_models(&self) {
        let model_ids: Vec<ModelId> = {
            let models = self.loaded_models.read().await;
            models.keys().copied().collect()
        };
        
        for model_id in model_ids {
            self.unregister_model(model_id).await;
        }
        
        // Reset usage counter to 0
        self.current_usage.store(0, Ordering::SeqCst);
        
        tracing::info!("Evicted all models");
    }
    
    /// Get information about all loaded models
    pub async fn get_loaded_models(&self) -> Vec<ModelInfo> {
        let models = self.loaded_models.read().await;
        models.values().cloned().collect()
    }
    
    /// Get the maximum VRAM limit in bytes
    pub fn max_vram_bytes(&self) -> u64 {
        self.max_vram_bytes
    }
    
    /// Set a new VRAM limit (in MB)
    /// Note: This doesn't automatically evict models if over the new limit
    pub fn set_max_vram_mb(&mut self, max_mb: u64) {
        self.max_vram_bytes = max_mb * 1024 * 1024;
    }
    
    /// Check if VRAM usage is above a threshold (0.0 - 1.0)
    pub fn is_above_threshold(&self, threshold: f32) -> bool {
        let status = self.get_status();
        status.usage_percent > threshold
    }
    
    /// Prewarm models (placeholder for future implementation)
    pub async fn prewarm_models(&self) -> Result<(), String> {
        // This would be implemented to preload frequently used models
        // during idle time
        tracing::debug!("Prewarm models called (not yet implemented)");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_vram_manager_basic() {
        let manager = VRAMManager::new(4096); // 4GB
        
        let status = manager.get_status();
        assert_eq!(status.used_bytes, 0);
        assert_eq!(status.max_bytes, 4096 * 1024 * 1024);
        assert_eq!(status.loaded_models, 0);
    }
    
    #[tokio::test]
    async fn test_register_unregister_model() {
        let manager = VRAMManager::new(4096);
        
        let model_info = ModelInfo {
            id: ModelId::from_type(super::super::config::ModelType::TextEmbedding),
            name: "test_model".to_string(),
            vram_bytes: 256 * 1024 * 1024, // 256MB
            last_used: Instant::now(),
            use_count: 0,
        };
        
        manager.register_model(model_info.clone()).await;
        
        let status = manager.get_status();
        assert_eq!(status.used_bytes, 256 * 1024 * 1024);
        assert_eq!(status.loaded_models, 1);
        
        manager.unregister_model(model_info.id).await;
        
        let status = manager.get_status();
        assert_eq!(status.used_bytes, 0);
        assert_eq!(status.loaded_models, 0);
    }
    
    #[tokio::test]
    async fn test_vram_limit_check() {
        let manager = VRAMManager::new(1024); // 1GB
        
        // Should have space for 512MB
        assert!(manager.has_available_vram(512 * 1024 * 1024));
        
        // Should have space for 1GB
        assert!(manager.has_available_vram(1024 * 1024 * 1024));
        
        // Should NOT have space for 2GB
        assert!(!manager.has_available_vram(2048 * 1024 * 1024));
    }
}
