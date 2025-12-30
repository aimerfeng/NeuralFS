//! Text Embedder using all-MiniLM-L6-v2 model
//!
//! Implements text embedding generation using ONNX Runtime.
//! Supports batch embedding for efficiency.

use std::sync::Arc;
use ndarray::{Array1, Array2, Axis};
use ort::Value;

use super::config::TextEmbeddingConfig;
use super::error::{EmbeddingError, EmbeddingResult};
use super::model_manager::ModelHandle;

/// Text embedder using transformer models
pub struct TextEmbedder {
    /// Model handle
    model_handle: Arc<ModelHandle>,
    
    /// Configuration
    config: TextEmbeddingConfig,
}

impl TextEmbedder {
    /// Create a new text embedder with the given model handle
    pub fn new(model_handle: Arc<ModelHandle>, config: TextEmbeddingConfig) -> EmbeddingResult<Self> {
        Ok(Self {
            model_handle,
            config,
        })
    }
    
    /// Embed a single text string
    pub async fn embed(&self, text: &str) -> EmbeddingResult<Vec<f32>> {
        let embeddings = self.batch_embed(&[text]).await?;
        Ok(embeddings.into_iter().next().unwrap_or_default())
    }
    
    /// Embed multiple text strings in a batch
    pub async fn batch_embed(&self, texts: &[&str]) -> EmbeddingResult<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }
        
        // Tokenize all texts
        let tokenized = self.tokenize_batch(texts)?;
        
        // Run inference
        let embeddings = self.run_inference(tokenized).await?;
        
        Ok(embeddings)
    }
    
    /// Simple tokenization (word-piece style)
    /// Note: In production, this should use a proper tokenizer like tokenizers-rs
    fn tokenize_batch(&self, texts: &[&str]) -> EmbeddingResult<TokenizedBatch> {
        let batch_size = texts.len();
        let max_len = self.config.max_seq_length;
        
        // Initialize arrays
        let mut input_ids = vec![vec![0i64; max_len]; batch_size];
        let mut attention_mask = vec![vec![0i64; max_len]; batch_size];
        let mut token_type_ids = vec![vec![0i64; max_len]; batch_size];
        
        for (i, text) in texts.iter().enumerate() {
            // Simple tokenization: split by whitespace and assign IDs
            // In production, use proper BPE/WordPiece tokenizer
            let tokens: Vec<&str> = text.split_whitespace().collect();
            
            // [CLS] token
            input_ids[i][0] = 101; // CLS token ID
            attention_mask[i][0] = 1;
            
            // Add tokens (simplified - just use hash as token ID)
            let token_count = tokens.len().min(max_len - 2);
            for (j, token) in tokens.iter().take(token_count).enumerate() {
                // Simple hash-based token ID (placeholder for real tokenizer)
                let token_id = self.simple_token_id(token);
                input_ids[i][j + 1] = token_id;
                attention_mask[i][j + 1] = 1;
            }
            
            // [SEP] token
            let sep_pos = token_count + 1;
            if sep_pos < max_len {
                input_ids[i][sep_pos] = 102; // SEP token ID
                attention_mask[i][sep_pos] = 1;
            }
        }
        
        Ok(TokenizedBatch {
            input_ids,
            attention_mask,
            token_type_ids,
        })
    }
    
    /// Simple token ID generation (placeholder for real tokenizer)
    fn simple_token_id(&self, token: &str) -> i64 {
        // Use a simple hash to generate token IDs
        // In production, use proper vocabulary lookup
        let hash: u64 = token.bytes().fold(0u64, |acc, b| {
            acc.wrapping_mul(31).wrapping_add(b as u64)
        });
        // Map to vocabulary range (assume 30000 vocab size)
        (hash % 30000) as i64 + 1000
    }
    
    /// Run inference on tokenized input
    async fn run_inference(&self, batch: TokenizedBatch) -> EmbeddingResult<Vec<Vec<f32>>> {
        let session = self.model_handle.session.clone();
        let embedding_dim = self.config.embedding_dim;
        
        // Clone data for the blocking task
        let input_ids = batch.input_ids;
        let attention_mask = batch.attention_mask;
        let token_type_ids = batch.token_type_ids;
        
        // Run inference in blocking task
        let result = tokio::task::spawn_blocking(move || {
            Self::run_inference_sync(
                &session,
                input_ids,
                attention_mask,
                token_type_ids,
                embedding_dim,
            )
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
        input_ids: Vec<Vec<i64>>,
        attention_mask: Vec<Vec<i64>>,
        token_type_ids: Vec<Vec<i64>>,
        embedding_dim: usize,
    ) -> EmbeddingResult<Vec<Vec<f32>>> {
        let batch_size = input_ids.len();
        let seq_len = input_ids[0].len();
        
        // Convert to ndarray
        let input_ids_flat: Vec<i64> = input_ids.into_iter().flatten().collect();
        let attention_mask_flat: Vec<i64> = attention_mask.into_iter().flatten().collect();
        let token_type_ids_flat: Vec<i64> = token_type_ids.into_iter().flatten().collect();
        
        let input_ids_array = Array2::from_shape_vec((batch_size, seq_len), input_ids_flat)
            .map_err(|e| EmbeddingError::InferenceFailed {
                reason: format!("Failed to create input_ids array: {}", e),
            })?;
        
        let attention_mask_array = Array2::from_shape_vec((batch_size, seq_len), attention_mask_flat)
            .map_err(|e| EmbeddingError::InferenceFailed {
                reason: format!("Failed to create attention_mask array: {}", e),
            })?;
        
        let token_type_ids_array = Array2::from_shape_vec((batch_size, seq_len), token_type_ids_flat)
            .map_err(|e| EmbeddingError::InferenceFailed {
                reason: format!("Failed to create token_type_ids array: {}", e),
            })?;
        
        // Create ONNX values
        let input_ids_value = Value::from_array(input_ids_array.view())
            .map_err(|e| EmbeddingError::InferenceFailed {
                reason: format!("Failed to create input_ids value: {}", e),
            })?;
        
        let attention_mask_value = Value::from_array(attention_mask_array.view())
            .map_err(|e| EmbeddingError::InferenceFailed {
                reason: format!("Failed to create attention_mask value: {}", e),
            })?;
        
        let token_type_ids_value = Value::from_array(token_type_ids_array.view())
            .map_err(|e| EmbeddingError::InferenceFailed {
                reason: format!("Failed to create token_type_ids value: {}", e),
            })?;
        
        // Run inference
        let outputs = session.run(ort::inputs![
            "input_ids" => input_ids_value,
            "attention_mask" => attention_mask_value,
            "token_type_ids" => token_type_ids_value,
        ].map_err(|e| EmbeddingError::InferenceFailed {
            reason: format!("Failed to create inputs: {}", e),
        })?)
        .map_err(|e| EmbeddingError::InferenceFailed {
            reason: format!("Inference failed: {}", e),
        })?;
        
        // Extract embeddings from output
        // The output is typically [batch_size, seq_len, hidden_size]
        // We need to pool to get [batch_size, hidden_size]
        let output = outputs.get("last_hidden_state")
            .or_else(|| outputs.get("sentence_embedding"))
            .or_else(|| outputs.iter().next().map(|(_, v)| v))
            .ok_or_else(|| EmbeddingError::InferenceFailed {
                reason: "No output found".to_string(),
            })?;
        
        // Try to extract as 3D array first (batch, seq, hidden)
        if let Ok(tensor) = output.try_extract_tensor::<f32>() {
            let shape = tensor.shape();
            
            if shape.len() == 3 {
                // [batch_size, seq_len, hidden_size] - need to pool
                let embeddings = Self::mean_pool_3d(&tensor, batch_size, embedding_dim);
                return Ok(embeddings);
            } else if shape.len() == 2 {
                // [batch_size, hidden_size] - already pooled
                let embeddings = Self::extract_2d(&tensor, batch_size, embedding_dim);
                return Ok(embeddings);
            }
        }
        
        // Fallback: return zero embeddings
        tracing::warn!("Could not extract embeddings from model output, returning zeros");
        Ok(vec![vec![0.0; embedding_dim]; batch_size])
    }
    
    /// Mean pooling for 3D tensor [batch, seq, hidden]
    fn mean_pool_3d(tensor: &ndarray::ArrayViewD<f32>, batch_size: usize, embedding_dim: usize) -> Vec<Vec<f32>> {
        let mut embeddings = Vec::with_capacity(batch_size);
        
        let shape = tensor.shape();
        let seq_len = shape.get(1).copied().unwrap_or(1);
        let hidden_size = shape.get(2).copied().unwrap_or(embedding_dim);
        
        for b in 0..batch_size {
            let mut embedding = vec![0.0f32; hidden_size.min(embedding_dim)];
            
            // Mean pool over sequence dimension
            for s in 0..seq_len {
                for h in 0..hidden_size.min(embedding_dim) {
                    if let Some(&val) = tensor.get([b, s, h]) {
                        embedding[h] += val / seq_len as f32;
                    }
                }
            }
            
            // Normalize
            let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 1e-8 {
                for x in &mut embedding {
                    *x /= norm;
                }
            }
            
            embeddings.push(embedding);
        }
        
        embeddings
    }
    
    /// Extract embeddings from 2D tensor [batch, hidden]
    fn extract_2d(tensor: &ndarray::ArrayViewD<f32>, batch_size: usize, embedding_dim: usize) -> Vec<Vec<f32>> {
        let mut embeddings = Vec::with_capacity(batch_size);
        
        let shape = tensor.shape();
        let hidden_size = shape.get(1).copied().unwrap_or(embedding_dim);
        
        for b in 0..batch_size {
            let mut embedding = vec![0.0f32; hidden_size.min(embedding_dim)];
            
            for h in 0..hidden_size.min(embedding_dim) {
                if let Some(&val) = tensor.get([b, h]) {
                    embedding[h] = val;
                }
            }
            
            // Normalize
            let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 1e-8 {
                for x in &mut embedding {
                    *x /= norm;
                }
            }
            
            embeddings.push(embedding);
        }
        
        embeddings
    }
    
    /// Get the embedding dimension
    pub fn embedding_dim(&self) -> usize {
        self.config.embedding_dim
    }
    
    /// Get the maximum sequence length
    pub fn max_seq_length(&self) -> usize {
        self.config.max_seq_length
    }
}

/// Tokenized batch ready for inference
struct TokenizedBatch {
    input_ids: Vec<Vec<i64>>,
    attention_mask: Vec<Vec<i64>>,
    token_type_ids: Vec<Vec<i64>>,
}
