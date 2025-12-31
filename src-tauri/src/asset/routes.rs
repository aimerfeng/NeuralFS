//! Asset server routes and middleware
//!
//! Provides HTTP handlers for serving thumbnails, previews, and files
//! with security middleware for token validation and CSRF protection.

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use uuid::Uuid;

use super::server::AssetServerState;

/// Query parameters for token-based authentication
#[derive(Debug, Deserialize)]
pub struct TokenParams {
    /// Session token (optional in query, can also be in header)
    pub token: Option<String>,
}

/// Security middleware for token validation and CSRF protection
///
/// This middleware:
/// 1. Validates the session token (from query param or X-Session-Token header)
/// 2. Validates the Origin header against allowed origins
/// 3. Validates the Referer header against allowed origins
/// 4. Adds security response headers
pub async fn security_middleware(
    State(state): State<AssetServerState>,
    Query(params): Query<TokenParams>,
    headers: HeaderMap,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Skip validation for health check endpoint
    if request.uri().path() == "/health" {
        return Ok(next.run(request).await);
    }

    // 1. Validate session token
    let token = params.token.as_deref().or_else(|| {
        headers
            .get("X-Session-Token")
            .and_then(|v| v.to_str().ok())
    });

    match token {
        Some(t) if state.validate_token(t) => {
            // Token is valid
        }
        _ => {
            tracing::warn!(
                "Invalid session token from {:?}, path: {}",
                headers.get("Origin"),
                request.uri().path()
            );
            return Err(StatusCode::FORBIDDEN);
        }
    }

    // 2. Validate Origin header (CSRF protection)
    if let Some(origin) = headers.get("Origin") {
        let origin_str = origin.to_str().unwrap_or("");
        if !state.is_origin_allowed(origin_str) {
            tracing::warn!("Blocked request from origin: {}", origin_str);
            return Err(StatusCode::FORBIDDEN);
        }
    }

    // 3. Validate Referer header (CSRF protection)
    if let Some(referer) = headers.get("Referer") {
        let referer_str = referer.to_str().unwrap_or("");
        if !state.is_referer_allowed(referer_str) {
            tracing::warn!("Blocked request with referer: {}", referer_str);
            return Err(StatusCode::FORBIDDEN);
        }
    }

    // 4. Continue processing the request
    let mut response = next.run(request).await;

    // 5. Add security response headers
    let headers = response.headers_mut();
    
    // Prevent MIME type sniffing
    headers.insert(
        "X-Content-Type-Options",
        HeaderValue::from_static("nosniff"),
    );
    
    // Prevent clickjacking
    headers.insert(
        "X-Frame-Options",
        HeaderValue::from_static("DENY"),
    );
    
    // Private caching only (no CDN caching)
    headers.insert(
        "Cache-Control",
        HeaderValue::from_static("private, max-age=3600"),
    );
    
    // Prevent XSS attacks
    headers.insert(
        "X-XSS-Protection",
        HeaderValue::from_static("1; mode=block"),
    );
    
    // Content Security Policy
    headers.insert(
        "Content-Security-Policy",
        HeaderValue::from_static("default-src 'self'"),
    );

    Ok(response)
}

/// Serve a thumbnail by UUID
///
/// Route: GET /thumbnail/:uuid
pub async fn serve_thumbnail(
    State(state): State<AssetServerState>,
    Path(uuid): Path<Uuid>,
) -> impl IntoResponse {
    match state.get_thumbnail(&uuid) {
        Some(cached) => {
            let headers = [
                (header::CONTENT_TYPE, cached.content_type.clone()),
                (header::CACHE_CONTROL, "private, max-age=3600".to_string()),
            ];
            (StatusCode::OK, headers, cached.data.as_ref().clone())
        }
        None => {
            let headers = [
                (header::CONTENT_TYPE, "text/plain".to_string()),
                (header::CACHE_CONTROL, "no-cache".to_string()),
            ];
            (StatusCode::NOT_FOUND, headers, b"Thumbnail not found".to_vec())
        }
    }
}

/// Serve a preview by UUID
///
/// Route: GET /preview/:uuid
pub async fn serve_preview(
    State(state): State<AssetServerState>,
    Path(uuid): Path<Uuid>,
) -> impl IntoResponse {
    match state.get_preview(&uuid) {
        Some(cached) => {
            let headers = [
                (header::CONTENT_TYPE, cached.content_type.clone()),
                (header::CACHE_CONTROL, "private, max-age=3600".to_string()),
            ];
            (StatusCode::OK, headers, cached.data.as_ref().clone())
        }
        None => {
            let headers = [
                (header::CONTENT_TYPE, "text/plain".to_string()),
                (header::CACHE_CONTROL, "no-cache".to_string()),
            ];
            (StatusCode::NOT_FOUND, headers, b"Preview not found".to_vec())
        }
    }
}

/// Serve a file by UUID
///
/// Route: GET /file/:uuid
///
/// Note: This is a placeholder. In a full implementation, this would:
/// 1. Look up the file path from the database by UUID
/// 2. Stream the file content with appropriate headers
/// 3. Support range requests for large files
pub async fn serve_file(
    State(_state): State<AssetServerState>,
    Path(uuid): Path<Uuid>,
) -> impl IntoResponse {
    // TODO: Implement file serving with database lookup
    // For now, return not implemented
    let headers = [
        (header::CONTENT_TYPE, "text/plain".to_string()),
    ];
    (
        StatusCode::NOT_IMPLEMENTED,
        headers,
        format!("File serving not yet implemented for UUID: {}", uuid).into_bytes(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::server::{AssetServerConfig, CachedThumbnail};

    #[test]
    fn test_token_params_deserialization() {
        let params: TokenParams = serde_json::from_str(r#"{"token": "abc123"}"#).unwrap();
        assert_eq!(params.token, Some("abc123".to_string()));

        let params: TokenParams = serde_json::from_str(r#"{}"#).unwrap();
        assert!(params.token.is_none());
    }

    #[test]
    fn test_state_token_validation() {
        let config = AssetServerConfig::with_port(19283);
        let state = AssetServerState::new(config);
        
        // Valid token should pass
        assert!(state.validate_token(&state.session_token));
        
        // Invalid token should fail
        assert!(!state.validate_token("invalid_token"));
        assert!(!state.validate_token(""));
    }

    #[test]
    fn test_state_origin_validation() {
        let config = AssetServerConfig::with_port(19283);
        let state = AssetServerState::new(config);
        
        // Allowed origins should pass
        assert!(state.is_origin_allowed("http://localhost:19283"));
        assert!(state.is_origin_allowed("http://127.0.0.1:19283"));
        assert!(state.is_origin_allowed("tauri://localhost"));
        
        // Disallowed origins should fail
        assert!(!state.is_origin_allowed("http://evil.com"));
        assert!(!state.is_origin_allowed("http://localhost:8080"));
    }

    #[test]
    fn test_state_referer_validation() {
        let config = AssetServerConfig::with_port(19283);
        let state = AssetServerState::new(config);
        
        // Allowed referers should pass
        assert!(state.is_referer_allowed("http://localhost:19283/page"));
        assert!(state.is_referer_allowed("http://127.0.0.1:19283/some/path"));
        assert!(state.is_referer_allowed("tauri://localhost/index.html"));
        
        // Disallowed referers should fail
        assert!(!state.is_referer_allowed("http://evil.com/page"));
        assert!(!state.is_referer_allowed("http://localhost:8080/page"));
    }

    #[test]
    fn test_thumbnail_caching() {
        let config = AssetServerConfig::with_port(19283);
        let state = AssetServerState::new(config);
        let uuid = Uuid::new_v4();
        
        // Initially empty
        assert!(state.get_thumbnail(&uuid).is_none());
        
        // Add thumbnail
        let thumbnail = CachedThumbnail::new(
            vec![1, 2, 3, 4],
            "image/png".to_string(),
            uuid,
        );
        state.cache_thumbnail(uuid, thumbnail);
        
        // Should be retrievable
        let cached = state.get_thumbnail(&uuid).unwrap();
        assert_eq!(cached.data.as_ref(), &vec![1, 2, 3, 4]);
        assert_eq!(cached.content_type, "image/png");
        
        // Invalidate
        state.invalidate_thumbnail(&uuid);
        assert!(state.get_thumbnail(&uuid).is_none());
    }
}
