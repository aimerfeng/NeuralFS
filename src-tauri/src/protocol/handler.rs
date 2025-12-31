//! Custom Protocol Handler Implementation
//!
//! Provides the `nfs://` protocol handler for serving assets securely.
//! This integrates with Tauri's custom protocol system and the existing
//! SecureAssetStreamServer for consistent security and caching.

use std::sync::Arc;
use tauri::http::{Request, Response, ResponseBuilder};
use uuid::Uuid;

use crate::asset::{AssetServerState, AssetServerConfig};

/// Protocol configuration
#[derive(Clone, Debug)]
pub struct ProtocolConfig {
    /// Whether to enable the custom protocol
    pub enabled: bool,
    /// Asset server configuration (shared with HTTP server)
    pub asset_config: AssetServerConfig,
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            asset_config: AssetServerConfig::default(),
        }
    }
}

/// Shared state for the protocol handler
#[derive(Clone)]
pub struct ProtocolState {
    /// Asset server state (shared with HTTP server for consistent caching)
    pub asset_state: Arc<AssetServerState>,
}

impl ProtocolState {
    /// Create a new protocol state with the given asset server state
    pub fn new(asset_state: AssetServerState) -> Self {
        Self {
            asset_state: Arc::new(asset_state),
        }
    }

    /// Create a new protocol state with default configuration
    pub fn with_default_config() -> Self {
        let config = AssetServerConfig::default();
        Self::new(AssetServerState::new(config))
    }

    /// Get the session token
    pub fn get_session_token(&self) -> &str {
        &self.asset_state.session_token
    }

    /// Validate a session token
    pub fn validate_token(&self, token: &str) -> bool {
        self.asset_state.validate_token(token)
    }
}

/// Asset protocol handler for `nfs://` URLs
///
/// Handles requests in the format:
/// - `nfs://thumbnail/{uuid}?token={session_token}`
/// - `nfs://preview/{uuid}?token={session_token}`
/// - `nfs://file/{uuid}?token={session_token}`
pub struct AssetProtocolHandler {
    state: ProtocolState,
}

impl AssetProtocolHandler {
    /// Create a new asset protocol handler
    pub fn new(state: ProtocolState) -> Self {
        Self { state }
    }

    /// Handle a protocol request
    pub fn handle(&self, request: &Request) -> Response {
        let uri = request.uri();
        let path = uri.path();
        let query = uri.query().unwrap_or("");

        // Parse token from query string
        let token = Self::parse_token_from_query(query);

        // Also check for token in headers (X-Session-Token)
        let header_token = request
            .headers()
            .get("X-Session-Token")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let effective_token = token.or(header_token);

        // Validate token
        match effective_token {
            Some(ref t) if self.state.validate_token(t) => {
                // Token is valid, proceed with request
            }
            _ => {
                tracing::warn!("Invalid or missing session token for path: {}", path);
                return Self::forbidden_response("Invalid or missing session token");
            }
        }

        // Route the request based on path
        // Path format: /thumbnail/{uuid}, /preview/{uuid}, /file/{uuid}
        let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();

        if parts.len() < 2 {
            return Self::not_found_response("Invalid path format");
        }

        let route = parts[0];
        let uuid_str = parts[1];

        // Parse UUID
        let uuid = match Uuid::parse_str(uuid_str) {
            Ok(u) => u,
            Err(_) => {
                return Self::bad_request_response(&format!("Invalid UUID: {}", uuid_str));
            }
        };

        // Handle route
        match route {
            "thumbnail" => self.serve_thumbnail(uuid),
            "preview" => self.serve_preview(uuid),
            "file" => self.serve_file(uuid),
            "health" => Self::health_response(),
            _ => Self::not_found_response(&format!("Unknown route: {}", route)),
        }
    }

    /// Parse token from query string
    fn parse_token_from_query(query: &str) -> Option<String> {
        for pair in query.split('&') {
            let mut parts = pair.splitn(2, '=');
            if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
                if key == "token" {
                    return Some(value.to_string());
                }
            }
        }
        None
    }

    /// Serve a thumbnail
    fn serve_thumbnail(&self, uuid: Uuid) -> Response {
        match self.state.asset_state.get_thumbnail(&uuid) {
            Some(cached) => {
                ResponseBuilder::new()
                    .status(200)
                    .header("Content-Type", &cached.content_type)
                    .header("Cache-Control", "private, max-age=3600")
                    .header("X-Content-Type-Options", "nosniff")
                    .body(cached.data.as_ref().clone())
                    .unwrap()
            }
            None => Self::not_found_response("Thumbnail not found"),
        }
    }

    /// Serve a preview
    fn serve_preview(&self, uuid: Uuid) -> Response {
        match self.state.asset_state.get_preview(&uuid) {
            Some(cached) => {
                ResponseBuilder::new()
                    .status(200)
                    .header("Content-Type", &cached.content_type)
                    .header("Cache-Control", "private, max-age=3600")
                    .header("X-Content-Type-Options", "nosniff")
                    .body(cached.data.as_ref().clone())
                    .unwrap()
            }
            None => Self::not_found_response("Preview not found"),
        }
    }

    /// Serve a file (placeholder - needs database integration)
    fn serve_file(&self, uuid: Uuid) -> Response {
        // TODO: Implement file serving with database lookup
        ResponseBuilder::new()
            .status(501)
            .header("Content-Type", "text/plain")
            .body(format!("File serving not yet implemented for UUID: {}", uuid).into_bytes())
            .unwrap()
    }

    /// Health check response
    fn health_response() -> Response {
        ResponseBuilder::new()
            .status(200)
            .header("Content-Type", "text/plain")
            .body(b"OK".to_vec())
            .unwrap()
    }

    /// 403 Forbidden response
    fn forbidden_response(message: &str) -> Response {
        ResponseBuilder::new()
            .status(403)
            .header("Content-Type", "text/plain")
            .body(message.as_bytes().to_vec())
            .unwrap()
    }

    /// 404 Not Found response
    fn not_found_response(message: &str) -> Response {
        ResponseBuilder::new()
            .status(404)
            .header("Content-Type", "text/plain")
            .body(message.as_bytes().to_vec())
            .unwrap()
    }

    /// 400 Bad Request response
    fn bad_request_response(message: &str) -> Response {
        ResponseBuilder::new()
            .status(400)
            .header("Content-Type", "text/plain")
            .body(message.as_bytes().to_vec())
            .unwrap()
    }
}

/// Register the `nfs://` custom protocol with Tauri
///
/// This function should be called during Tauri app setup to register
/// the custom protocol handler.
///
/// # Example
///
/// ```ignore
/// let protocol_state = ProtocolState::with_default_config();
/// 
/// tauri::Builder::default()
///     .setup(|app| {
///         register_custom_protocol(app, protocol_state.clone());
///         Ok(())
///     })
///     .manage(protocol_state)
///     .run(tauri::generate_context!())
///     .expect("error while running tauri application");
/// ```
pub fn register_custom_protocol(
    builder: tauri::Builder<tauri::Wry>,
    state: ProtocolState,
) -> tauri::Builder<tauri::Wry> {
    let handler = AssetProtocolHandler::new(state.clone());

    builder.register_uri_scheme_protocol("nfs", move |_app, request| {
        handler.handle(&request)
    })
}

/// Tauri command to get the session token
///
/// This command should be called by the frontend during app initialization
/// to obtain the session token for subsequent asset requests.
///
/// # Returns
///
/// A JSON object containing:
/// - `token`: The session token string
/// - `protocol_url`: The base URL for the nfs:// protocol
/// - `http_url`: The base URL for the HTTP asset server (fallback)
#[tauri::command]
pub fn get_session_token(state: tauri::State<ProtocolState>) -> SessionTokenResponse {
    SessionTokenResponse {
        token: state.get_session_token().to_string(),
        protocol_url: "nfs://".to_string(),
        http_url: format!("http://127.0.0.1:{}", state.asset_state.config.port),
    }
}

/// Response structure for get_session_token command
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SessionTokenResponse {
    /// The session token for authenticating asset requests
    pub token: String,
    /// Base URL for the nfs:// custom protocol
    pub protocol_url: String,
    /// Base URL for the HTTP asset server (fallback)
    pub http_url: String,
}

/// Helper function to build asset URLs with token
pub fn build_thumbnail_url(state: &ProtocolState, uuid: Uuid, use_protocol: bool) -> String {
    let token = state.get_session_token();
    if use_protocol {
        format!("nfs://thumbnail/{}?token={}", uuid, token)
    } else {
        format!(
            "http://127.0.0.1:{}/thumbnail/{}?token={}",
            state.asset_state.config.port, uuid, token
        )
    }
}

/// Helper function to build preview URLs with token
pub fn build_preview_url(state: &ProtocolState, uuid: Uuid, use_protocol: bool) -> String {
    let token = state.get_session_token();
    if use_protocol {
        format!("nfs://preview/{}?token={}", uuid, token)
    } else {
        format!(
            "http://127.0.0.1:{}/preview/{}?token={}",
            state.asset_state.config.port, uuid, token
        )
    }
}

/// Helper function to build file URLs with token
pub fn build_file_url(state: &ProtocolState, uuid: Uuid, use_protocol: bool) -> String {
    let token = state.get_session_token();
    if use_protocol {
        format!("nfs://file/{}?token={}", uuid, token)
    } else {
        format!(
            "http://127.0.0.1:{}/file/{}?token={}",
            state.asset_state.config.port, uuid, token
        )
    }
}
