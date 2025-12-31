//! Tests for the custom protocol handler

use super::*;
use crate::asset::{AssetServerConfig, AssetServerState, CachedThumbnail, CachedPreview};
use crate::asset::server::PreviewType;
use uuid::Uuid;

fn create_test_state() -> ProtocolState {
    let config = AssetServerConfig::with_port(19283);
    ProtocolState::new(AssetServerState::new(config))
}

#[test]
fn test_protocol_state_creation() {
    let state = create_test_state();
    
    // Token should be generated
    assert!(!state.get_session_token().is_empty());
    assert_eq!(state.get_session_token().len(), 64); // 32 bytes hex encoded
}

#[test]
fn test_token_validation() {
    let state = create_test_state();
    let token = state.get_session_token().to_string();
    
    // Valid token should pass
    assert!(state.validate_token(&token));
    
    // Invalid tokens should fail
    assert!(!state.validate_token("invalid_token"));
    assert!(!state.validate_token(""));
    assert!(!state.validate_token(&token[..token.len()-1])); // Truncated token
}

#[test]
fn test_parse_token_from_query() {
    // Token present
    let token = AssetProtocolHandler::parse_token_from_query("token=abc123");
    assert_eq!(token, Some("abc123".to_string()));
    
    // Token with other params
    let token = AssetProtocolHandler::parse_token_from_query("foo=bar&token=xyz789&baz=qux");
    assert_eq!(token, Some("xyz789".to_string()));
    
    // No token
    let token = AssetProtocolHandler::parse_token_from_query("foo=bar&baz=qux");
    assert!(token.is_none());
    
    // Empty query
    let token = AssetProtocolHandler::parse_token_from_query("");
    assert!(token.is_none());
}

#[test]
fn test_thumbnail_serving() {
    let state = create_test_state();
    let uuid = Uuid::new_v4();
    
    // Add a thumbnail to cache
    let thumbnail = CachedThumbnail::new(
        vec![0x89, 0x50, 0x4E, 0x47], // PNG magic bytes
        "image/png".to_string(),
        uuid,
    );
    state.asset_state.cache_thumbnail(uuid, thumbnail);
    
    // Create handler
    let handler = AssetProtocolHandler::new(state.clone());
    
    // Build a mock request
    let token = state.get_session_token();
    let uri = format!("/thumbnail/{}?token={}", uuid, token);
    let request = tauri::http::Request::builder()
        .uri(&uri)
        .body(Vec::new())
        .unwrap();
    
    // Handle request
    let response = handler.handle(&request);
    
    // Should return 200 with PNG data
    assert_eq!(response.status(), 200);
    assert_eq!(response.body(), &vec![0x89, 0x50, 0x4E, 0x47]);
}

#[test]
fn test_thumbnail_not_found() {
    let state = create_test_state();
    let uuid = Uuid::new_v4();
    
    let handler = AssetProtocolHandler::new(state.clone());
    
    let token = state.get_session_token();
    let uri = format!("/thumbnail/{}?token={}", uuid, token);
    let request = tauri::http::Request::builder()
        .uri(&uri)
        .body(Vec::new())
        .unwrap();
    
    let response = handler.handle(&request);
    
    // Should return 404
    assert_eq!(response.status(), 404);
}

#[test]
fn test_preview_serving() {
    let state = create_test_state();
    let uuid = Uuid::new_v4();
    
    // Add a preview to cache
    let preview = CachedPreview::new(
        b"Hello, World!".to_vec(),
        "text/plain".to_string(),
        uuid,
        PreviewType::Text,
    );
    state.asset_state.cache_preview(uuid, preview);
    
    let handler = AssetProtocolHandler::new(state.clone());
    
    let token = state.get_session_token();
    let uri = format!("/preview/{}?token={}", uuid, token);
    let request = tauri::http::Request::builder()
        .uri(&uri)
        .body(Vec::new())
        .unwrap();
    
    let response = handler.handle(&request);
    
    assert_eq!(response.status(), 200);
    assert_eq!(response.body(), b"Hello, World!");
}

#[test]
fn test_invalid_token_rejected() {
    let state = create_test_state();
    let uuid = Uuid::new_v4();
    
    let handler = AssetProtocolHandler::new(state);
    
    // Request with invalid token
    let uri = format!("/thumbnail/{}?token=invalid_token", uuid);
    let request = tauri::http::Request::builder()
        .uri(&uri)
        .body(Vec::new())
        .unwrap();
    
    let response = handler.handle(&request);
    
    // Should return 403 Forbidden
    assert_eq!(response.status(), 403);
}

#[test]
fn test_missing_token_rejected() {
    let state = create_test_state();
    let uuid = Uuid::new_v4();
    
    let handler = AssetProtocolHandler::new(state);
    
    // Request without token
    let uri = format!("/thumbnail/{}", uuid);
    let request = tauri::http::Request::builder()
        .uri(&uri)
        .body(Vec::new())
        .unwrap();
    
    let response = handler.handle(&request);
    
    // Should return 403 Forbidden
    assert_eq!(response.status(), 403);
}

#[test]
fn test_invalid_uuid_rejected() {
    let state = create_test_state();
    
    let handler = AssetProtocolHandler::new(state.clone());
    
    let token = state.get_session_token();
    let uri = format!("/thumbnail/not-a-uuid?token={}", token);
    let request = tauri::http::Request::builder()
        .uri(&uri)
        .body(Vec::new())
        .unwrap();
    
    let response = handler.handle(&request);
    
    // Should return 400 Bad Request
    assert_eq!(response.status(), 400);
}

#[test]
fn test_unknown_route() {
    let state = create_test_state();
    let uuid = Uuid::new_v4();
    
    let handler = AssetProtocolHandler::new(state.clone());
    
    let token = state.get_session_token();
    let uri = format!("/unknown/{}?token={}", uuid, token);
    let request = tauri::http::Request::builder()
        .uri(&uri)
        .body(Vec::new())
        .unwrap();
    
    let response = handler.handle(&request);
    
    // Should return 404 Not Found
    assert_eq!(response.status(), 404);
}

#[test]
fn test_health_endpoint() {
    let state = create_test_state();
    
    let handler = AssetProtocolHandler::new(state.clone());
    
    let token = state.get_session_token();
    let uri = format!("/health/check?token={}", token);
    let request = tauri::http::Request::builder()
        .uri(&uri)
        .body(Vec::new())
        .unwrap();
    
    let response = handler.handle(&request);
    
    assert_eq!(response.status(), 200);
    assert_eq!(response.body(), b"OK");
}

#[test]
fn test_build_thumbnail_url_protocol() {
    let state = create_test_state();
    let uuid = Uuid::new_v4();
    
    let url = build_thumbnail_url(&state, uuid, true);
    
    assert!(url.starts_with("nfs://thumbnail/"));
    assert!(url.contains(&uuid.to_string()));
    assert!(url.contains("token="));
}

#[test]
fn test_build_thumbnail_url_http() {
    let state = create_test_state();
    let uuid = Uuid::new_v4();
    
    let url = build_thumbnail_url(&state, uuid, false);
    
    assert!(url.starts_with("http://127.0.0.1:"));
    assert!(url.contains("/thumbnail/"));
    assert!(url.contains(&uuid.to_string()));
    assert!(url.contains("token="));
}

#[test]
fn test_build_preview_url() {
    let state = create_test_state();
    let uuid = Uuid::new_v4();
    
    let protocol_url = build_preview_url(&state, uuid, true);
    let http_url = build_preview_url(&state, uuid, false);
    
    assert!(protocol_url.starts_with("nfs://preview/"));
    assert!(http_url.starts_with("http://127.0.0.1:"));
    assert!(http_url.contains("/preview/"));
}

#[test]
fn test_build_file_url() {
    let state = create_test_state();
    let uuid = Uuid::new_v4();
    
    let protocol_url = build_file_url(&state, uuid, true);
    let http_url = build_file_url(&state, uuid, false);
    
    assert!(protocol_url.starts_with("nfs://file/"));
    assert!(http_url.starts_with("http://127.0.0.1:"));
    assert!(http_url.contains("/file/"));
}

#[test]
fn test_session_token_response_serialization() {
    let response = SessionTokenResponse {
        token: "test_token_123".to_string(),
        protocol_url: "nfs://".to_string(),
        http_url: "http://127.0.0.1:19283".to_string(),
    };
    
    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("test_token_123"));
    assert!(json.contains("nfs://"));
    assert!(json.contains("http://127.0.0.1:19283"));
    
    let deserialized: SessionTokenResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.token, response.token);
    assert_eq!(deserialized.protocol_url, response.protocol_url);
    assert_eq!(deserialized.http_url, response.http_url);
}
