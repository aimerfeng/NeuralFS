//! Embedding Engine for NeuralFS
//!
//! This module provides AI-powered embedding generation for semantic search.
//! It supports:
//! - Text embeddings using all-MiniLM-L6-v2 (384 dimensions)
//! - Image embeddings using CLIP model
//! - VRAM management with LRU model caching
//! - Graceful degradation when models are not ready
//! - Diluted attention for processing long documents

mod config;
mod error;
mod model_manager;
mod vram_manager;
mod text_embedder;
mod image_embedder;
mod diluted;

#[cfg(test)]
mod tests;

pub use config::{EmbeddingConfig, ModelConfig, ModelType};
pub use error::{EmbeddingError, EmbeddingResult};
pub use model_manager::{ModelManager, ModelHandle, ModelLoadingState, ModelId};
pub use vram_manager::{VRAMManager, VRAMStatus, ModelInfo};
pub use text_embedder::TextEmbedder;
pub use image_embedder::ImageEmbedder;
pub use diluted::{DilutedAttentionProcessor, DilutedAttentionConfig, ProcessedWindow, CoverageStats, Token};

use std::sync::Arc;
use tokio::sync::RwLock;

/// Main embedding engine that coordinates model loading and inference
pub struct EmbeddingEngine {
    /// Model manager for lazy loading and caching
    model_manager: Arc<ModelManager>,
    
    /// VRAM manager for memory tracking
    vram_manager: Arc<VRAMManager>,
    
    /// Text embedder instance
    text_embedder: Arc<RwLock<Option<TextEmbedder>>>,
    
    /// Image embedder instance
    image_embedder: Arc<RwLock<Option<ImageEmbedder>>>,
    
    /// Configuration
    config: EmbeddingConfig,
}

impl EmbeddingEngine {
    /// Create a new embedding engine with the given configuration
    pub fn new(config: EmbeddingConfig) -> Self {
        let vram_manager = Arc::new(VRAMManager::new(config.max_vram_mb));
        let model_manager = Arc::new(ModelManager::new(
            config.models_dir.clone(),
            vram_manager.clone(),
        ));
        
        Self {
            model_manager,
            vram_manager,
            text_embedder: Arc::new(RwLock::new(None)),
            image_embedder: Arc::new(RwLock::new(None)),
            config,
        }
    }
    
    /// Initialize the embedding engine (lazy - doesn't load models yet)
    pub async fn initialize(&self) -> EmbeddingResult<()> {
        tracing::info!("Embedding engine initialized (models will be loaded on first use)");
        Ok(())
    }
    
    /// Get the current state of a model
    pub async fn get_model_state(&self, model_type: ModelType) -> ModelLoadingState {
        self.model_manager.get_model_state(model_type).await
    }
    
    /// Embed text content using the text embedding model
    /// Returns empty vector if model is not ready (graceful degradation)
    pub async fn embed_text_content(&self, text: &str) -> EmbeddingResult<Vec<f32>> {
        // Check if text embedder is initialized
        let embedder = self.text_embedder.read().await;
        if let Some(ref embedder) = *embedder {
            return embedder.embed(text).await;
        }
        drop(embedder);
        
        // Try to initialize text embedder
        match self.ensure_text_embedder().await {
            Ok(()) => {
                let embedder = self.text_embedder.read().await;
                if let Some(ref embedder) = *embedder {
                    embedder.embed(text).await
                } else {
                    // Model not ready - return empty (graceful degradation)
                    tracing::warn!("Text embedding model not ready, returning empty embedding");
                    Ok(vec![])
                }
            }
            Err(e) => {
                tracing::warn!("Failed to load text embedding model: {}, returning empty embedding", e);
                Ok(vec![])
            }
        }
    }
    
    /// Batch embed multiple text contents
    /// Returns empty vectors for any texts that fail (graceful degradation)
    pub async fn batch_embed_text(&self, texts: &[&str]) -> EmbeddingResult<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }
        
        // Check if text embedder is initialized
        let embedder = self.text_embedder.read().await;
        if let Some(ref embedder) = *embedder {
            return embedder.batch_embed(texts).await;
        }
        drop(embedder);
        
        // Try to initialize text embedder
        match self.ensure_text_embedder().await {
            Ok(()) => {
                let embedder = self.text_embedder.read().await;
                if let Some(ref embedder) = *embedder {
                    embedder.batch_embed(texts).await
                } else {
                    // Model not ready - return empty vectors
                    tracing::warn!("Text embedding model not ready, returning empty embeddings");
                    Ok(vec![vec![]; texts.len()])
                }
            }
            Err(e) => {
                tracing::warn!("Failed to load text embedding model: {}, returning empty embeddings", e);
                Ok(vec![vec![]; texts.len()])
            }
        }
    }
    
    /// Embed image content using the CLIP model
    /// Returns empty vector if model is not ready (graceful degradation)
    pub async fn embed_image(&self, image_data: &[u8]) -> EmbeddingResult<Vec<f32>> {
        // Check if image embedder is initialized
        let embedder = self.image_embedder.read().await;
        if let Some(ref embedder) = *embedder {
            return embedder.embed(image_data).await;
        }
        drop(embedder);
        
        // Try to initialize image embedder
        match self.ensure_image_embedder().await {
            Ok(()) => {
                let embedder = self.image_embedder.read().await;
                if let Some(ref embedder) = *embedder {
                    embedder.embed(image_data).await
                } else {
                    tracing::warn!("Image embedding model not ready, returning empty embedding");
                    Ok(vec![])
                }
            }
            Err(e) => {
                tracing::warn!("Failed to load image embedding model: {}, returning empty embedding", e);
                Ok(vec![])
            }
        }
    }
    
    /// Embed image from file path
    pub async fn embed_image_file(&self, path: &std::path::Path) -> EmbeddingResult<Vec<f32>> {
        let image_data = tokio::fs::read(path).await.map_err(|e| {
            EmbeddingError::ImageProcessingFailed {
                reason: format!("Failed to read image file: {}", e),
            }
        })?;
        self.embed_image(&image_data).await
    }
    
    /// Get current VRAM status
    pub fn get_vram_status(&self) -> VRAMStatus {
        self.vram_manager.get_status()
    }
    
    /// Unload all models to free VRAM
    pub async fn unload_all_models(&self) -> EmbeddingResult<()> {
        // Clear text embedder
        {
            let mut embedder = self.text_embedder.write().await;
            *embedder = None;
        }
        
        // Clear image embedder
        {
            let mut embedder = self.image_embedder.write().await;
            *embedder = None;
        }
        
        // Evict all models from VRAM manager
        self.vram_manager.evict_all_models().await;
        
        tracing::info!("All embedding models unloaded");
        Ok(())
    }
    
    /// Ensure text embedder is initialized
    async fn ensure_text_embedder(&self) -> EmbeddingResult<()> {
        let mut embedder = self.text_embedder.write().await;
        if embedder.is_some() {
            return Ok(());
        }
        
        // Load the text embedding model
        let model_handle = self.model_manager
            .load_model(ModelType::TextEmbedding)
            .await?;
        
        let text_embedder = TextEmbedder::new(model_handle, self.config.text_config.clone())?;
        *embedder = Some(text_embedder);
        
        Ok(())
    }
    
    /// Ensure image embedder is initialized
    async fn ensure_image_embedder(&self) -> EmbeddingResult<()> {
        let mut embedder = self.image_embedder.write().await;
        if embedder.is_some() {
            return Ok(());
        }
        
        // Load the image embedding model
        let model_handle = self.model_manager
            .load_model(ModelType::ImageEmbedding)
            .await?;
        
        let image_embedder = ImageEmbedder::new(model_handle, self.config.image_config.clone())?;
        *embedder = Some(image_embedder);
        
        Ok(())
    }
}

impl Default for EmbeddingEngine {
    fn default() -> Self {
        Self::new(EmbeddingConfig::default())
    }
}
