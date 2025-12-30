//! Configuration for the embedding engine

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// Main configuration for the embedding engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Directory containing ONNX model files
    pub models_dir: PathBuf,
    
    /// Maximum VRAM usage in MB (default: 4096 = 4GB)
    pub max_vram_mb: u64,
    
    /// Text embedding configuration
    pub text_config: TextEmbeddingConfig,
    
    /// Image embedding configuration
    pub image_config: ImageEmbeddingConfig,
    
    /// Whether to use GPU acceleration
    pub use_gpu: bool,
    
    /// Batch size for embedding operations
    pub batch_size: usize,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            models_dir: PathBuf::from("models"),
            max_vram_mb: 4096, // 4GB default limit
            text_config: TextEmbeddingConfig::default(),
            image_config: ImageEmbeddingConfig::default(),
            use_gpu: true,
            batch_size: 32,
        }
    }
}

/// Configuration for text embedding model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEmbeddingConfig {
    /// Model filename (relative to models_dir)
    pub model_file: String,
    
    /// Tokenizer vocabulary file
    pub vocab_file: String,
    
    /// Maximum sequence length
    pub max_seq_length: usize,
    
    /// Embedding dimension (384 for MiniLM-L6)
    pub embedding_dim: usize,
    
    /// Estimated VRAM usage in MB
    pub vram_mb: u64,
}

impl Default for TextEmbeddingConfig {
    fn default() -> Self {
        Self {
            model_file: "all-MiniLM-L6-v2.onnx".to_string(),
            vocab_file: "vocab.txt".to_string(),
            max_seq_length: 256,
            embedding_dim: 384,
            vram_mb: 256, // ~256MB for MiniLM-L6
        }
    }
}

/// Configuration for image embedding model (CLIP)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageEmbeddingConfig {
    /// Model filename (relative to models_dir)
    pub model_file: String,
    
    /// Input image size (width and height)
    pub image_size: u32,
    
    /// Embedding dimension (512 for CLIP ViT-B/32)
    pub embedding_dim: usize,
    
    /// Estimated VRAM usage in MB
    pub vram_mb: u64,
}

impl Default for ImageEmbeddingConfig {
    fn default() -> Self {
        Self {
            model_file: "clip-vit-base-patch32.onnx".to_string(),
            image_size: 224,
            embedding_dim: 512,
            vram_mb: 512, // ~512MB for CLIP ViT-B/32
        }
    }
}

/// Model type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelType {
    /// Text embedding model (all-MiniLM-L6-v2)
    TextEmbedding,
    
    /// Image embedding model (CLIP)
    ImageEmbedding,
    
    /// Fast text model for quick inference
    FastText,
    
    /// Accurate text model for high-quality embeddings
    AccurateText,
}

impl ModelType {
    /// Get the default model filename for this type
    pub fn default_filename(&self) -> &'static str {
        match self {
            ModelType::TextEmbedding => "all-MiniLM-L6-v2.onnx",
            ModelType::ImageEmbedding => "clip-vit-base-patch32.onnx",
            ModelType::FastText => "all-MiniLM-L6-v2.onnx",
            ModelType::AccurateText => "bge-base-en-v1.5.onnx",
        }
    }
    
    /// Get the embedding dimension for this model type
    pub fn embedding_dim(&self) -> usize {
        match self {
            ModelType::TextEmbedding => 384,
            ModelType::ImageEmbedding => 512,
            ModelType::FastText => 384,
            ModelType::AccurateText => 768,
        }
    }
    
    /// Get estimated VRAM usage in MB
    pub fn estimated_vram_mb(&self) -> u64 {
        match self {
            ModelType::TextEmbedding => 256,
            ModelType::ImageEmbedding => 512,
            ModelType::FastText => 256,
            ModelType::AccurateText => 512,
        }
    }
}

/// Model configuration for a specific model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Model type
    pub model_type: ModelType,
    
    /// Model file path
    pub model_path: PathBuf,
    
    /// Embedding dimension
    pub embedding_dim: usize,
    
    /// Maximum input length (for text models)
    pub max_input_length: Option<usize>,
    
    /// Input image size (for image models)
    pub input_image_size: Option<u32>,
    
    /// Estimated VRAM usage in MB
    pub vram_mb: u64,
    
    /// Whether to use GPU
    pub use_gpu: bool,
}

impl ModelConfig {
    /// Create a new model config for text embedding
    pub fn text_embedding(models_dir: &PathBuf, use_gpu: bool) -> Self {
        Self {
            model_type: ModelType::TextEmbedding,
            model_path: models_dir.join("all-MiniLM-L6-v2.onnx"),
            embedding_dim: 384,
            max_input_length: Some(256),
            input_image_size: None,
            vram_mb: 256,
            use_gpu,
        }
    }
    
    /// Create a new model config for image embedding
    pub fn image_embedding(models_dir: &PathBuf, use_gpu: bool) -> Self {
        Self {
            model_type: ModelType::ImageEmbedding,
            model_path: models_dir.join("clip-vit-base-patch32.onnx"),
            embedding_dim: 512,
            max_input_length: None,
            input_image_size: Some(224),
            vram_mb: 512,
            use_gpu,
        }
    }
}
