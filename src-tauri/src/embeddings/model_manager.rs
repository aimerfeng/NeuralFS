//! Model Manager for lazy loading and caching ONNX models
//!
//! Implements:
//! - Lazy model loading on first use
//! - Model state machine (Missing, Downloading, Loading, Ready, Failed)
//! - LRU caching with VRAM management

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use ort::{Environment, Session, SessionBuilder, ExecutionProvider};

use super::config::ModelType;
use super::error::{EmbeddingError, EmbeddingResult};
use super::vram_manager::{VRAMManager, ModelInfo};

/// Unique identifier for a loaded model
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModelId(u64);

impl ModelId {
    /// Create a new model ID from a model type
    pub fn from_type(model_type: ModelType) -> Self {
        ModelId(model_type as u64)
    }
}

/// State of a model in the loading pipeline
#[derive(Debug, Clone, PartialEq)]
pub enum ModelLoadingState {
    /// Model file is missing and needs to be downloaded
    Missing,
    
    /// Model is being downloaded
    Downloading { progress: f32 },
    
    /// Model is being loaded into memory
    Loading,
    
    /// Model is ready for inference
    Ready,
    
    /// Model loading failed
    Failed { reason: String },
}

impl ModelLoadingState {
    /// Check if the model is ready for inference
    pub fn is_ready(&self) -> bool {
        matches!(self, ModelLoadingState::Ready)
    }
    
    /// Check if the model is in a loading state
    pub fn is_loading(&self) -> bool {
        matches!(
            self,
            ModelLoadingState::Downloading { .. } | ModelLoadingState::Loading
        )
    }
    
    /// Check if the model failed to load
    pub fn is_failed(&self) -> bool {
        matches!(self, ModelLoadingState::Failed { .. })
    }
}

/// Handle to a loaded ONNX model session
pub struct ModelHandle {
    /// The ONNX session
    pub session: Arc<Session>,
    
    /// Model type
    pub model_type: ModelType,
    
    /// Model ID
    pub id: ModelId,
    
    /// When the model was loaded
    pub loaded_at: Instant,
    
    /// Number of times this model has been used
    pub use_count: u64,
}

impl ModelHandle {
    /// Record a use of this model
    pub fn record_use(&mut self) {
        self.use_count += 1;
    }
}

/// Internal state for a model
struct ModelState {
    state: ModelLoadingState,
    handle: Option<Arc<ModelHandle>>,
    last_error: Option<String>,
}

/// Model Manager for loading and caching ONNX models
pub struct ModelManager {
    /// Directory containing model files
    models_dir: PathBuf,
    
    /// VRAM manager for memory tracking
    vram_manager: Arc<VRAMManager>,
    
    /// ONNX Runtime environment
    environment: Arc<Environment>,
    
    /// Model states
    model_states: RwLock<HashMap<ModelType, ModelState>>,
    
    /// Loaded model handles
    loaded_models: RwLock<HashMap<ModelId, Arc<ModelHandle>>>,
}

impl ModelManager {
    /// Create a new model manager
    pub fn new(models_dir: PathBuf, vram_manager: Arc<VRAMManager>) -> Self {
        // Initialize ONNX Runtime environment
        let environment = Arc::new(
            Environment::builder()
                .with_name("NeuralFS")
                .with_execution_providers([
                    // Try CUDA first, fall back to CPU
                    #[cfg(feature = "cuda")]
                    ExecutionProvider::CUDA(Default::default()),
                    ExecutionProvider::CPU(Default::default()),
                ])
                .build()
                .expect("Failed to create ONNX Runtime environment")
        );
        
        Self {
            models_dir,
            vram_manager,
            environment,
            model_states: RwLock::new(HashMap::new()),
            loaded_models: RwLock::new(HashMap::new()),
        }
    }
    
    /// Get the current state of a model
    pub async fn get_model_state(&self, model_type: ModelType) -> ModelLoadingState {
        let states = self.model_states.read().await;
        states
            .get(&model_type)
            .map(|s| s.state.clone())
            .unwrap_or_else(|| {
                // Check if model file exists
                let model_path = self.get_model_path(model_type);
                if model_path.exists() {
                    ModelLoadingState::Ready // File exists but not loaded yet
                } else {
                    ModelLoadingState::Missing
                }
            })
    }
    
    /// Load a model (lazy loading with state machine)
    pub async fn load_model(&self, model_type: ModelType) -> EmbeddingResult<Arc<ModelHandle>> {
        let model_id = ModelId::from_type(model_type);
        
        // Check if already loaded
        {
            let loaded = self.loaded_models.read().await;
            if let Some(handle) = loaded.get(&model_id) {
                return Ok(handle.clone());
            }
        }
        
        // Check current state
        let current_state = self.get_model_state(model_type).await;
        
        match current_state {
            ModelLoadingState::Ready => {
                // Model file exists, load it
                self.do_load_model(model_type).await
            }
            ModelLoadingState::Missing => {
                // Model file doesn't exist
                Err(EmbeddingError::ModelNotFound {
                    path: self.get_model_path(model_type).to_string_lossy().to_string(),
                })
            }
            ModelLoadingState::Loading | ModelLoadingState::Downloading { .. } => {
                // Model is being loaded, return error (caller should retry)
                Err(EmbeddingError::ModelLoading {
                    model_type: format!("{:?}", model_type),
                })
            }
            ModelLoadingState::Failed { reason } => {
                Err(EmbeddingError::ModelFailed {
                    model_type: format!("{:?}", model_type),
                    reason,
                })
            }
        }
    }
    
    /// Actually load the model into memory
    async fn do_load_model(&self, model_type: ModelType) -> EmbeddingResult<Arc<ModelHandle>> {
        let model_id = ModelId::from_type(model_type);
        let model_path = self.get_model_path(model_type);
        
        // Update state to Loading
        {
            let mut states = self.model_states.write().await;
            states.insert(model_type, ModelState {
                state: ModelLoadingState::Loading,
                handle: None,
                last_error: None,
            });
        }
        
        // Check VRAM availability
        let vram_needed = model_type.estimated_vram_mb();
        let vram_status = self.vram_manager.get_status();
        let available = vram_status.max_bytes.saturating_sub(vram_status.used_bytes) / (1024 * 1024);
        
        if available < vram_needed {
            // Try to evict models to make room
            if let Err(e) = self.vram_manager.evict_models(vram_needed * 1024 * 1024).await {
                let error_msg = format!("Failed to free VRAM: {}", e);
                self.set_model_failed(model_type, &error_msg).await;
                return Err(EmbeddingError::VRAMInsufficient {
                    needed_mb: vram_needed,
                    available_mb: available,
                });
            }
        }
        
        // Load the ONNX model
        let session = match self.create_session(&model_path).await {
            Ok(session) => session,
            Err(e) => {
                let error_msg = e.to_string();
                self.set_model_failed(model_type, &error_msg).await;
                return Err(e);
            }
        };
        
        // Create model handle
        let handle = Arc::new(ModelHandle {
            session: Arc::new(session),
            model_type,
            id: model_id,
            loaded_at: Instant::now(),
            use_count: 0,
        });
        
        // Register with VRAM manager
        let model_info = ModelInfo {
            id: model_id,
            name: format!("{:?}", model_type),
            vram_bytes: vram_needed * 1024 * 1024,
            last_used: Instant::now(),
            use_count: 0,
        };
        self.vram_manager.register_model(model_info).await;
        
        // Store the handle
        {
            let mut loaded = self.loaded_models.write().await;
            loaded.insert(model_id, handle.clone());
        }
        
        // Update state to Ready
        {
            let mut states = self.model_states.write().await;
            states.insert(model_type, ModelState {
                state: ModelLoadingState::Ready,
                handle: Some(handle.clone()),
                last_error: None,
            });
        }
        
        tracing::info!("Loaded model {:?} from {:?}", model_type, model_path);
        Ok(handle)
    }
    
    /// Create an ONNX session for a model
    async fn create_session(&self, model_path: &PathBuf) -> EmbeddingResult<Session> {
        if !model_path.exists() {
            return Err(EmbeddingError::ModelNotFound {
                path: model_path.to_string_lossy().to_string(),
            });
        }
        
        // Clone values for the blocking task
        let path = model_path.clone();
        let env = self.environment.clone();
        
        // Load model in blocking task to avoid blocking async runtime
        let session = tokio::task::spawn_blocking(move || {
            SessionBuilder::new(&env)?
                .with_optimization_level(ort::GraphOptimizationLevel::Level3)?
                .with_intra_threads(4)?
                .with_model_from_file(&path)
        })
        .await
        .map_err(|e| EmbeddingError::ModelLoadFailed {
            reason: format!("Task join error: {}", e),
        })??;
        
        Ok(session)
    }
    
    /// Set a model to failed state
    async fn set_model_failed(&self, model_type: ModelType, reason: &str) {
        let mut states = self.model_states.write().await;
        states.insert(model_type, ModelState {
            state: ModelLoadingState::Failed {
                reason: reason.to_string(),
            },
            handle: None,
            last_error: Some(reason.to_string()),
        });
    }
    
    /// Get the path to a model file
    fn get_model_path(&self, model_type: ModelType) -> PathBuf {
        self.models_dir.join(model_type.default_filename())
    }
    
    /// Unload a specific model
    pub async fn unload_model(&self, model_type: ModelType) -> EmbeddingResult<()> {
        let model_id = ModelId::from_type(model_type);
        
        // Remove from loaded models
        {
            let mut loaded = self.loaded_models.write().await;
            loaded.remove(&model_id);
        }
        
        // Update state
        {
            let mut states = self.model_states.write().await;
            states.remove(&model_type);
        }
        
        // Unregister from VRAM manager
        self.vram_manager.unregister_model(model_id).await;
        
        tracing::info!("Unloaded model {:?}", model_type);
        Ok(())
    }
    
    /// Get a loaded model handle
    pub async fn get_model(&self, model_type: ModelType) -> Option<Arc<ModelHandle>> {
        let model_id = ModelId::from_type(model_type);
        let loaded = self.loaded_models.read().await;
        loaded.get(&model_id).cloned()
    }
    
    /// Check if a model is loaded
    pub async fn is_model_loaded(&self, model_type: ModelType) -> bool {
        let model_id = ModelId::from_type(model_type);
        let loaded = self.loaded_models.read().await;
        loaded.contains_key(&model_id)
    }
    
    /// Get all loaded model types
    pub async fn get_loaded_models(&self) -> Vec<ModelType> {
        let loaded = self.loaded_models.read().await;
        loaded.values().map(|h| h.model_type).collect()
    }
}
