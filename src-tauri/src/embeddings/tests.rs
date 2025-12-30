//! Tests for the embedding engine

use super::*;
use std::path::PathBuf;
use tempfile::TempDir;

#[cfg(test)]
mod config_tests {
    use super::*;
    
    #[test]
    fn test_default_config() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.max_vram_mb, 4096);
        assert!(config.use_gpu);
        assert_eq!(config.batch_size, 32);
    }
    
    #[test]
    fn test_model_type_properties() {
        assert_eq!(ModelType::TextEmbedding.embedding_dim(), 384);
        assert_eq!(ModelType::ImageEmbedding.embedding_dim(), 512);
        assert_eq!(ModelType::FastText.embedding_dim(), 384);
        assert_eq!(ModelType::AccurateText.embedding_dim(), 768);
    }
    
    #[test]
    fn test_model_type_vram() {
        assert_eq!(ModelType::TextEmbedding.estimated_vram_mb(), 256);
        assert_eq!(ModelType::ImageEmbedding.estimated_vram_mb(), 512);
    }
}

#[cfg(test)]
mod vram_manager_tests {
    use super::*;
    use std::time::Instant;
    
    #[tokio::test]
    async fn test_vram_manager_creation() {
        let manager = VRAMManager::new(4096);
        let status = manager.get_status();
        
        assert_eq!(status.used_bytes, 0);
        assert_eq!(status.max_bytes, 4096 * 1024 * 1024);
        assert_eq!(status.loaded_models, 0);
        assert_eq!(status.usage_percent, 0.0);
    }
    
    #[tokio::test]
    async fn test_register_model() {
        let manager = VRAMManager::new(4096);
        
        let model_info = ModelInfo {
            id: ModelId::from_type(ModelType::TextEmbedding),
            name: "test".to_string(),
            vram_bytes: 256 * 1024 * 1024,
            last_used: Instant::now(),
            use_count: 0,
        };
        
        manager.register_model(model_info).await;
        
        let status = manager.get_status();
        assert_eq!(status.used_bytes, 256 * 1024 * 1024);
        assert_eq!(status.loaded_models, 1);
    }
    
    #[tokio::test]
    async fn test_unregister_model() {
        let manager = VRAMManager::new(4096);
        let model_id = ModelId::from_type(ModelType::TextEmbedding);
        
        let model_info = ModelInfo {
            id: model_id,
            name: "test".to_string(),
            vram_bytes: 256 * 1024 * 1024,
            last_used: Instant::now(),
            use_count: 0,
        };
        
        manager.register_model(model_info).await;
        manager.unregister_model(model_id).await;
        
        let status = manager.get_status();
        assert_eq!(status.used_bytes, 0);
        assert_eq!(status.loaded_models, 0);
    }
    
    #[tokio::test]
    async fn test_has_available_vram() {
        let manager = VRAMManager::new(1024); // 1GB
        
        assert!(manager.has_available_vram(512 * 1024 * 1024));
        assert!(manager.has_available_vram(1024 * 1024 * 1024));
        assert!(!manager.has_available_vram(2048 * 1024 * 1024));
    }
    
    #[tokio::test]
    async fn test_evict_all_models() {
        let manager = VRAMManager::new(4096);
        
        // Register multiple models
        for i in 0..3 {
            let model_info = ModelInfo {
                id: ModelId(i),
                name: format!("model_{}", i),
                vram_bytes: 256 * 1024 * 1024,
                last_used: Instant::now(),
                use_count: 0,
            };
            manager.register_model(model_info).await;
        }
        
        let status = manager.get_status();
        assert_eq!(status.loaded_models, 3);
        
        manager.evict_all_models().await;
        
        let status = manager.get_status();
        assert_eq!(status.used_bytes, 0);
        assert_eq!(status.loaded_models, 0);
    }
    
    #[tokio::test]
    async fn test_record_model_use() {
        let manager = VRAMManager::new(4096);
        let model_id = ModelId::from_type(ModelType::TextEmbedding);
        
        let model_info = ModelInfo {
            id: model_id,
            name: "test".to_string(),
            vram_bytes: 256 * 1024 * 1024,
            last_used: Instant::now(),
            use_count: 0,
        };
        
        manager.register_model(model_info).await;
        manager.record_model_use(model_id).await;
        
        let models = manager.get_loaded_models().await;
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].use_count, 1);
    }
}

#[cfg(test)]
mod model_loading_state_tests {
    use super::*;
    
    #[test]
    fn test_state_is_ready() {
        assert!(ModelLoadingState::Ready.is_ready());
        assert!(!ModelLoadingState::Missing.is_ready());
        assert!(!ModelLoadingState::Loading.is_ready());
    }
    
    #[test]
    fn test_state_is_loading() {
        assert!(ModelLoadingState::Loading.is_loading());
        assert!(ModelLoadingState::Downloading { progress: 0.5 }.is_loading());
        assert!(!ModelLoadingState::Ready.is_loading());
        assert!(!ModelLoadingState::Missing.is_loading());
    }
    
    #[test]
    fn test_state_is_failed() {
        assert!(ModelLoadingState::Failed { reason: "test".to_string() }.is_failed());
        assert!(!ModelLoadingState::Ready.is_failed());
        assert!(!ModelLoadingState::Loading.is_failed());
    }
}

#[cfg(test)]
mod error_tests {
    use super::*;
    
    #[test]
    fn test_error_retryable() {
        let err = EmbeddingError::ModelLoading { model_type: "test".to_string() };
        assert!(err.is_retryable());
        
        let err = EmbeddingError::ModelNotFound { path: "test".to_string() };
        assert!(!err.is_retryable());
        
        let err = EmbeddingError::VRAMInsufficient { needed_mb: 512, available_mb: 256 };
        assert!(err.is_retryable());
    }
    
    #[test]
    fn test_error_should_degrade() {
        let err = EmbeddingError::ModelNotFound { path: "test".to_string() };
        assert!(err.should_degrade());
        
        let err = EmbeddingError::ModelNotLoaded { model_type: "test".to_string() };
        assert!(err.should_degrade());
        
        let err = EmbeddingError::InferenceFailed { reason: "test".to_string() };
        assert!(!err.should_degrade());
    }
}

#[cfg(test)]
mod embedding_engine_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_engine_creation() {
        let config = EmbeddingConfig::default();
        let engine = EmbeddingEngine::new(config);
        
        let status = engine.get_vram_status();
        assert_eq!(status.used_bytes, 0);
    }
    
    #[tokio::test]
    async fn test_engine_graceful_degradation_text() {
        // Create engine with non-existent models directory
        let config = EmbeddingConfig {
            models_dir: PathBuf::from("/nonexistent/path"),
            ..Default::default()
        };
        let engine = EmbeddingEngine::new(config);
        
        // Should return empty embedding instead of error (graceful degradation)
        let result = engine.embed_text_content("test text").await;
        assert!(result.is_ok());
        let embedding = result.unwrap();
        assert!(embedding.is_empty()); // Empty due to model not found
    }
    
    #[tokio::test]
    async fn test_engine_batch_embed_empty() {
        let config = EmbeddingConfig::default();
        let engine = EmbeddingEngine::new(config);
        
        let result = engine.batch_embed_text(&[]).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }
    
    #[tokio::test]
    async fn test_engine_unload_all() {
        let config = EmbeddingConfig::default();
        let engine = EmbeddingEngine::new(config);
        
        let result = engine.unload_all_models().await;
        assert!(result.is_ok());
        
        let status = engine.get_vram_status();
        assert_eq!(status.used_bytes, 0);
        assert_eq!(status.loaded_models, 0);
    }
}

// Property-based tests for VRAM management
#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;
    use std::time::Instant;
    
    proptest! {
        /// Property 6: VRAM Usage Bound
        /// For any sequence of model registrations, the VRAM usage SHALL not exceed the configured limit
        #[test]
        fn prop_vram_usage_bound(
            max_vram_mb in 1024u64..8192u64,
            model_sizes in prop::collection::vec(64u64..1024u64, 1..10)
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let manager = VRAMManager::new(max_vram_mb);
                let max_bytes = max_vram_mb * 1024 * 1024;
                
                // Register models
                for (i, size_mb) in model_sizes.iter().enumerate() {
                    let model_info = ModelInfo {
                        id: ModelId(i as u64),
                        name: format!("model_{}", i),
                        vram_bytes: size_mb * 1024 * 1024,
                        last_used: Instant::now(),
                        use_count: 0,
                    };
                    
                    // Only register if it fits
                    if manager.has_available_vram(model_info.vram_bytes) {
                        manager.register_model(model_info).await;
                    }
                }
                
                // Verify VRAM usage is within bounds
                let status = manager.get_status();
                prop_assert!(status.used_bytes <= max_bytes,
                    "VRAM usage {} exceeds limit {}", status.used_bytes, max_bytes);
            });
        }
        
        /// Property: VRAM tracking is consistent after register/unregister cycles
        #[test]
        fn prop_vram_tracking_consistent(
            operations in prop::collection::vec((0u64..5u64, bool), 1..20)
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let manager = VRAMManager::new(4096);
                let model_size = 256u64 * 1024 * 1024; // 256MB per model
                
                let mut registered: std::collections::HashSet<u64> = std::collections::HashSet::new();
                
                for (model_idx, should_register) in operations {
                    let model_id = ModelId(model_idx);
                    
                    if should_register && !registered.contains(&model_idx) {
                        let model_info = ModelInfo {
                            id: model_id,
                            name: format!("model_{}", model_idx),
                            vram_bytes: model_size,
                            last_used: Instant::now(),
                            use_count: 0,
                        };
                        manager.register_model(model_info).await;
                        registered.insert(model_idx);
                    } else if !should_register && registered.contains(&model_idx) {
                        manager.unregister_model(model_id).await;
                        registered.remove(&model_idx);
                    }
                }
                
                // Verify consistency
                let status = manager.get_status();
                let expected_usage = registered.len() as u64 * model_size;
                prop_assert_eq!(status.used_bytes, expected_usage,
                    "VRAM tracking inconsistent: expected {}, got {}", expected_usage, status.used_bytes);
                prop_assert_eq!(status.loaded_models, registered.len(),
                    "Model count inconsistent: expected {}, got {}", registered.len(), status.loaded_models);
            });
        }
    }
}
