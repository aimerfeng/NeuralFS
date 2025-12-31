//! Secure Asset Stream Server implementation
//!
//! Provides a localhost HTTP server for streaming assets with security features:
//! - Session token validation
//! - CSRF protection
//! - Security headers

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use axum::{
    Router,
    routing::get,
    middleware,
    http::{HeaderValue, Method},
};
use dashmap::DashMap;
use tower_http::cors::{CorsLayer, Any};
use uuid::Uuid;

use super::error::AssetError;
use super::routes::{serve_thumbnail, serve_preview, serve_file, security_middleware};

/// Default port for the asset server
pub const DEFAULT_ASSET_SERVER_PORT: u16 = 19283;

/// Cached thumbnail data
#[derive(Clone)]
pub struct CachedThumbnail {
    /// Raw image data
    pub data: Arc<Vec<u8>>,
    /// Content type (MIME type)
    pub content_type: String,
    /// When this entry was cached
    pub created_at: Instant,
    /// Original file UUID
    pub file_id: Uuid,
}

impl CachedThumbnail {
    /// Create a new cached thumbnail
    pub fn new(data: Vec<u8>, content_type: String, file_id: Uuid) -> Self {
        Self {
            data: Arc::new(data),
            content_type,
            created_at: Instant::now(),
            file_id,
        }
    }

    /// Get the size of the cached data in bytes
    pub fn size(&self) -> usize {
        self.data.len()
    }
}

/// Cached preview data
#[derive(Clone)]
pub struct CachedPreview {
    /// Raw preview data
    pub data: Arc<Vec<u8>>,
    /// Content type (MIME type)
    pub content_type: String,
    /// When this entry was cached
    pub created_at: Instant,
    /// Original file UUID
    pub file_id: Uuid,
    /// Preview type (text, image, document)
    pub preview_type: PreviewType,
}

/// Preview type enumeration
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewType {
    Text,
    Image,
    Document,
    Code,
}

impl CachedPreview {
    /// Create a new cached preview
    pub fn new(data: Vec<u8>, content_type: String, file_id: Uuid, preview_type: PreviewType) -> Self {
        Self {
            data: Arc::new(data),
            content_type,
            created_at: Instant::now(),
            file_id,
            preview_type,
        }
    }

    /// Get the size of the cached data in bytes
    pub fn size(&self) -> usize {
        self.data.len()
    }
}

/// Asset server configuration
#[derive(Clone, Debug)]
pub struct AssetServerConfig {
    /// Port to bind to (localhost only)
    pub port: u16,
    /// Additional allowed origins (beyond localhost)
    pub additional_origins: Vec<String>,
    /// Maximum cache size in bytes (0 = unlimited)
    pub max_cache_size: usize,
    /// Cache TTL in seconds
    pub cache_ttl_secs: u64,
}

impl Default for AssetServerConfig {
    fn default() -> Self {
        Self {
            port: DEFAULT_ASSET_SERVER_PORT,
            additional_origins: Vec::new(),
            max_cache_size: 100 * 1024 * 1024, // 100 MB
            cache_ttl_secs: 3600, // 1 hour
        }
    }
}

impl AssetServerConfig {
    /// Create a new configuration with a custom port
    pub fn with_port(port: u16) -> Self {
        Self {
            port,
            ..Default::default()
        }
    }

    /// Get all allowed origins
    pub fn allowed_origins(&self) -> Vec<String> {
        let mut origins = vec![
            format!("http://localhost:{}", self.port),
            format!("http://127.0.0.1:{}", self.port),
            "tauri://localhost".to_string(),
            "https://tauri.localhost".to_string(),
        ];
        origins.extend(self.additional_origins.clone());
        origins
    }
}

/// Shared state for the asset server
#[derive(Clone)]
pub struct AssetServerState {
    /// Session token (generated at startup)
    pub session_token: String,
    /// Thumbnail cache
    pub thumbnail_cache: Arc<DashMap<Uuid, CachedThumbnail>>,
    /// Preview cache
    pub preview_cache: Arc<DashMap<Uuid, CachedPreview>>,
    /// Allowed origins for CSRF protection
    pub allowed_origins: Vec<String>,
    /// Configuration
    pub config: AssetServerConfig,
}

impl AssetServerState {
    /// Create a new server state with the given configuration
    pub fn new(config: AssetServerConfig) -> Self {
        let session_token = Self::generate_session_token();
        let allowed_origins = config.allowed_origins();

        Self {
            session_token,
            thumbnail_cache: Arc::new(DashMap::new()),
            preview_cache: Arc::new(DashMap::new()),
            allowed_origins,
            config,
        }
    }

    /// Generate a cryptographically secure session token
    fn generate_session_token() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let bytes: [u8; 32] = rng.gen();
        hex::encode(bytes)
    }

    /// Validate a session token
    pub fn validate_token(&self, token: &str) -> bool {
        // Constant-time comparison to prevent timing attacks
        self.session_token.len() == token.len() 
            && self.session_token.as_bytes()
                .iter()
                .zip(token.as_bytes())
                .fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
    }

    /// Check if an origin is allowed
    pub fn is_origin_allowed(&self, origin: &str) -> bool {
        self.allowed_origins.iter().any(|o| o == origin)
    }

    /// Check if a referer is allowed
    pub fn is_referer_allowed(&self, referer: &str) -> bool {
        self.allowed_origins.iter().any(|o| referer.starts_with(o))
    }

    /// Add a thumbnail to the cache
    pub fn cache_thumbnail(&self, uuid: Uuid, thumbnail: CachedThumbnail) {
        self.thumbnail_cache.insert(uuid, thumbnail);
    }

    /// Get a thumbnail from the cache
    pub fn get_thumbnail(&self, uuid: &Uuid) -> Option<CachedThumbnail> {
        self.thumbnail_cache.get(uuid).map(|entry| entry.clone())
    }

    /// Add a preview to the cache
    pub fn cache_preview(&self, uuid: Uuid, preview: CachedPreview) {
        self.preview_cache.insert(uuid, preview);
    }

    /// Get a preview from the cache
    pub fn get_preview(&self, uuid: &Uuid) -> Option<CachedPreview> {
        self.preview_cache.get(uuid).map(|entry| entry.clone())
    }

    /// Remove a thumbnail from the cache
    pub fn invalidate_thumbnail(&self, uuid: &Uuid) {
        self.thumbnail_cache.remove(uuid);
    }

    /// Remove a preview from the cache
    pub fn invalidate_preview(&self, uuid: &Uuid) {
        self.preview_cache.remove(uuid);
    }

    /// Clear all caches
    pub fn clear_caches(&self) {
        self.thumbnail_cache.clear();
        self.preview_cache.clear();
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> CacheStats {
        let thumbnail_count = self.thumbnail_cache.len();
        let thumbnail_size: usize = self.thumbnail_cache
            .iter()
            .map(|entry| entry.size())
            .sum();

        let preview_count = self.preview_cache.len();
        let preview_size: usize = self.preview_cache
            .iter()
            .map(|entry| entry.size())
            .sum();

        CacheStats {
            thumbnail_count,
            thumbnail_size_bytes: thumbnail_size,
            preview_count,
            preview_size_bytes: preview_size,
            total_size_bytes: thumbnail_size + preview_size,
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Number of cached thumbnails
    pub thumbnail_count: usize,
    /// Total size of cached thumbnails in bytes
    pub thumbnail_size_bytes: usize,
    /// Number of cached previews
    pub preview_count: usize,
    /// Total size of cached previews in bytes
    pub preview_size_bytes: usize,
    /// Total cache size in bytes
    pub total_size_bytes: usize,
}

/// Secure Asset Stream Server
///
/// Provides a localhost HTTP server for streaming assets with security features:
/// - Session token validation (token must be provided via query param or header)
/// - CSRF protection via Origin/Referer checking
/// - Security response headers
pub struct SecureAssetStreamServer {
    /// Server state (shared with handlers)
    state: AssetServerState,
}

impl SecureAssetStreamServer {
    /// Create a new secure asset stream server with default configuration
    pub fn new(port: u16) -> Self {
        let config = AssetServerConfig::with_port(port);
        Self {
            state: AssetServerState::new(config),
        }
    }

    /// Create a new secure asset stream server with custom configuration
    pub fn with_config(config: AssetServerConfig) -> Self {
        Self {
            state: AssetServerState::new(config),
        }
    }

    /// Get the session token (for frontend to use in requests)
    pub fn get_session_token(&self) -> &str {
        &self.state.session_token
    }

    /// Get the server port
    pub fn port(&self) -> u16 {
        self.state.config.port
    }

    /// Get a reference to the server state
    pub fn state(&self) -> &AssetServerState {
        &self.state
    }

    /// Get a clone of the server state (for sharing with handlers)
    pub fn state_clone(&self) -> AssetServerState {
        self.state.clone()
    }

    /// Build the router with all routes and middleware
    pub fn build_router(&self) -> Router {
        let state = self.state.clone();

        // Configure CORS
        let cors = CorsLayer::new()
            .allow_methods([Method::GET, Method::OPTIONS])
            .allow_headers(Any)
            .allow_origin(
                self.state.allowed_origins
                    .iter()
                    .filter_map(|o| o.parse::<HeaderValue>().ok())
                    .collect::<Vec<_>>()
            );

        Router::new()
            .route("/thumbnail/:uuid", get(serve_thumbnail))
            .route("/preview/:uuid", get(serve_preview))
            .route("/file/:uuid", get(serve_file))
            .route("/health", get(|| async { "OK" }))
            .layer(middleware::from_fn_with_state(state.clone(), security_middleware))
            .layer(cors)
            .with_state(state)
    }

    /// Start the server (blocking)
    pub async fn start(&self) -> Result<(), AssetError> {
        let addr = SocketAddr::from(([127, 0, 0, 1], self.state.config.port));
        let router = self.build_router();

        tracing::info!(
            "Secure asset server listening on {} with token {}...",
            addr,
            &self.state.session_token[..8]
        );

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| AssetError::BindFailed { reason: e.to_string() })?;

        axum::serve(listener, router)
            .await
            .map_err(|e| AssetError::Internal { reason: e.to_string() })?;

        Ok(())
    }

    /// Start the server in a background task
    pub fn start_background(self) -> tokio::task::JoinHandle<Result<(), AssetError>> {
        tokio::spawn(async move {
            self.start().await
        })
    }

    // Cache management methods (delegate to state)

    /// Add a thumbnail to the cache
    pub fn cache_thumbnail(&self, uuid: Uuid, thumbnail: CachedThumbnail) {
        self.state.cache_thumbnail(uuid, thumbnail);
    }

    /// Get a thumbnail from the cache
    pub fn get_thumbnail(&self, uuid: &Uuid) -> Option<CachedThumbnail> {
        self.state.get_thumbnail(uuid)
    }

    /// Add a preview to the cache
    pub fn cache_preview(&self, uuid: Uuid, preview: CachedPreview) {
        self.state.cache_preview(uuid, preview);
    }

    /// Get a preview from the cache
    pub fn get_preview(&self, uuid: &Uuid) -> Option<CachedPreview> {
        self.state.get_preview(uuid)
    }

    /// Invalidate a thumbnail in the cache
    pub fn invalidate_thumbnail(&self, uuid: &Uuid) {
        self.state.invalidate_thumbnail(uuid);
    }

    /// Invalidate a preview in the cache
    pub fn invalidate_preview(&self, uuid: &Uuid) {
        self.state.invalidate_preview(uuid);
    }

    /// Clear all caches
    pub fn clear_caches(&self) {
        self.state.clear_caches();
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> CacheStats {
        self.state.cache_stats()
    }

    /// Get the thumbnail URL for a file
    pub fn get_thumbnail_url(&self, uuid: Uuid) -> String {
        format!(
            "http://127.0.0.1:{}/thumbnail/{}?token={}",
            self.state.config.port,
            uuid,
            self.state.session_token
        )
    }

    /// Get the preview URL for a file
    pub fn get_preview_url(&self, uuid: Uuid) -> String {
        format!(
            "http://127.0.0.1:{}/preview/{}?token={}",
            self.state.config.port,
            uuid,
            self.state.session_token
        )
    }

    /// Get the file URL for a file
    pub fn get_file_url(&self, uuid: Uuid) -> String {
        format!(
            "http://127.0.0.1:{}/file/{}?token={}",
            self.state.config.port,
            uuid,
            self.state.session_token
        )
    }
}
