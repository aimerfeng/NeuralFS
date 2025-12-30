//! Image Embedder using CLIP model
//!
//! Implements image embedding generation using ONNX Runtime.
//! Supports CLIP ViT-B/32 model for visual embeddings.

use std::sync::Arc;
use image::{DynamicImage, GenericImageView, imageops::FilterType};
use ndarray::Array4;
use ort::Value;

use super::config::ImageEmbeddingConfig;
use super::error::{EmbeddingError, EmbeddingResult};
use super::model_manager::ModelHandle;

/// Image embedder using CLIP model
pub struct ImageEmbedder {
    /// Model handle
    model_handle: Arc<ModelHandle>,
    
    /// Configuration
    config: ImageEmbeddingConfig,
}

impl ImageEmbedder {
    /// Create a new image embedder with the given model handle
    pub fn new(model_handle: Arc<ModelHandle>, config: ImageEmbeddingConfig) -> EmbeddingResult<Self> {
        Ok(Self {
            model_handle,
            config,
        })
    }
    
    /// Embed image from raw bytes
    pub async fn embed(&self, image_data: &[u8]) -> EmbeddingResult<Vec<f32>> {
        // Load and preprocess image
        let preprocessed = self.preprocess_image(image_data)?;
        
        // Run inference
        let embedding = self.run_inference(preprocessed).await?;
        
        Ok(embedding)
    }
    
    /// Embed image from DynamicImage
    pub async fn embed_image(&self, image: &DynamicImage) -> EmbeddingResult<Vec<f32>> {
        let preprocessed = self.preprocess_dynamic_image(image)?;
        self.run_inference(preprocessed).await
    }
    
    /// Preprocess image from raw bytes
    fn preprocess_image(&self, image_data: &[u8]) -> EmbeddingResult<PreprocessedImage> {
        // Load image
        let image = image::load_from_memory(image_data).map_err(|e| {
            EmbeddingError::ImageProcessingFailed {
                reason: format!("Failed to load image: {}", e),
            }
        })?;
        
        self.preprocess_dynamic_image(&image)
    }
    
    /// Preprocess a DynamicImage for CLIP
    fn preprocess_dynamic_image(&self, image: &DynamicImage) -> EmbeddingResult<PreprocessedImage> {
        let target_size = self.config.image_size;
        
        // Resize image to target size
        let resized = image.resize_exact(target_size, target_size, FilterType::Lanczos3);
        
        // Convert to RGB
        let rgb = resized.to_rgb8();
        
        // Normalize to CLIP expected format
        // CLIP uses ImageNet normalization: mean=[0.48145466, 0.4578275, 0.40821073], std=[0.26862954, 0.26130258, 0.27577711]
        let mean = [0.48145466f32, 0.4578275, 0.40821073];
        let std = [0.26862954f32, 0.26130258, 0.27577711];
        
        let mut pixel_data = vec![0.0f32; 3 * (target_size as usize) * (target_size as usize)];
        
        for (x, y, pixel) in rgb.enumerate_pixels() {
            let idx = (y as usize) * (target_size as usize) + (x as usize);
            
            // Normalize each channel
            for c in 0..3 {
                let value = pixel[c] as f32 / 255.0;
                let normalized = (value - mean[c]) / std[c];
                // NCHW format: channel first
                pixel_data[c * (target_size as usize) * (target_size as usize) + idx] = normalized;
            }
        }
        
        Ok(PreprocessedImage {
            data: pixel_data,
            width: target_size,
            height: target_size,
            channels: 3,
        })
    }
    
    /// Run inference on preprocessed image
    async fn run_inference(&self, image: PreprocessedImage) -> EmbeddingResult<Vec<f32>> {
        let session = self.model_handle.session.clone();
        let embedding_dim = self.config.embedding_dim;
        
        // Run inference in blocking task
        let result = tokio::task::spawn_blocking(move || {
            Self::run_inference_sync(&session, image, embedding_dim)
        })
        .await
        .map_err(|e| EmbeddingError::InferenceFailed {
            reason: format!("Task join error: {}", e),
        })??;
        
        Ok(result)
    }
    
    /// Synchronous inference (runs in blocking task)
    fn run_inference_sync(
        session: &ort::Session,
        image: PreprocessedImage,
        embedding_dim: usize,
    ) -> EmbeddingResult<Vec<f32>> {
        // Create input tensor [1, 3, H, W]
        let input_array = Array4::from_shape_vec(
            (1, image.channels as usize, image.height as usize, image.width as usize),
            image.data,
        )
        .map_err(|e| EmbeddingError::InferenceFailed {
            reason: format!("Failed to create input array: {}", e),
        })?;
        
        // Create ONNX value
        let input_value = Value::from_array(input_array.view())
            .map_err(|e| EmbeddingError::InferenceFailed {
                reason: format!("Failed to create input value: {}", e),
            })?;
        
        // Run inference - try common input names
        let outputs = session.run(ort::inputs![
            "pixel_values" => input_value.clone(),
        ].or_else(|_| ort::inputs!["input" => input_value.clone()])
         .or_else(|_| ort::inputs!["image" => input_value])
         .map_err(|e| EmbeddingError::InferenceFailed {
            reason: format!("Failed to create inputs: {}", e),
        })?)
        .map_err(|e| EmbeddingError::InferenceFailed {
            reason: format!("Inference failed: {}", e),
        })?;
        
        // Extract embedding from output
        let output = outputs.get("image_embeds")
            .or_else(|| outputs.get("pooler_output"))
            .or_else(|| outputs.get("output"))
            .or_else(|| outputs.iter().next().map(|(_, v)| v))
            .ok_or_else(|| EmbeddingError::InferenceFailed {
                reason: "No output found".to_string(),
            })?;
        
        // Extract tensor
        if let Ok(tensor) = output.try_extract_tensor::<f32>() {
            let shape = tensor.shape();
            
            // Handle different output shapes
            let embedding = if shape.len() == 2 {
                // [batch, hidden] - take first batch
                let hidden_size = shape[1].min(embedding_dim);
                (0..hidden_size)
                    .map(|i| tensor.get([0, i]).copied().unwrap_or(0.0))
                    .collect()
            } else if shape.len() == 1 {
                // [hidden] - direct embedding
                let hidden_size = shape[0].min(embedding_dim);
                (0..hidden_size)
                    .map(|i| tensor.get([i]).copied().unwrap_or(0.0))
                    .collect()
            } else {
                // Unknown shape - return zeros
                vec![0.0; embedding_dim]
            };
            
            // Normalize embedding
            let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 1e-8 {
                return Ok(embedding.into_iter().map(|x| x / norm).collect());
            }
            
            return Ok(embedding);
        }
        
        // Fallback: return zero embedding
        tracing::warn!("Could not extract embedding from model output, returning zeros");
        Ok(vec![0.0; embedding_dim])
    }
    
    /// Get the embedding dimension
    pub fn embedding_dim(&self) -> usize {
        self.config.embedding_dim
    }
    
    /// Get the expected input image size
    pub fn image_size(&self) -> u32 {
        self.config.image_size
    }
}

/// Preprocessed image ready for inference
struct PreprocessedImage {
    /// Normalized pixel data in NCHW format
    data: Vec<f32>,
    /// Image width
    width: u32,
    /// Image height
    height: u32,
    /// Number of channels (3 for RGB)
    channels: u32,
}
