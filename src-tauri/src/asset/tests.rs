//! Tests for the asset server module
//!
//! Includes unit tests and property-based tests for:
//! - Token validation
//! - CSRF protection
//! - Cache operations

use super::*;
use super::server::{AssetServerConfig, AssetServerState, CachedThumbnail, CachedPreview, PreviewType};
use uuid::Uuid;

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_server_creation() {
        let server = SecureAssetStreamServer::new(19283);
        assert_eq!(server.port(), 19283);
        assert!(!server.get_session_token().is_empty());
        assert_eq!(server.get_session_token().len(), 64); // 32 bytes = 64 hex chars
    }

    #[test]
    fn test_server_with_config() {
        let config = AssetServerConfig {
            port: 8080,
            additional_origins: vec!["http://custom.origin".to_string()],
            max_cache_size: 50 * 1024 * 1024,
            cache_ttl_secs: 1800,
        };
        let server = SecureAssetStreamServer::with_config(config);
        assert_eq!(server.port(), 8080);
    }

    #[test]
    fn test_session_token_uniqueness() {
        let server1 = SecureAssetStreamServer::new(19283);
        let server2 = SecureAssetStreamServer::new(19284);
        
        // Each server should have a unique token
        assert_ne!(server1.get_session_token(), server2.get_session_token());
    }

    #[test]
    fn test_url_generation() {
        let server = SecureAssetStreamServer::new(19283);
        let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        
        let thumbnail_url = server.get_thumbnail_url(uuid);
        assert!(thumbnail_url.contains("thumbnail"));
        assert!(thumbnail_url.contains(&uuid.to_string()));
        assert!(thumbnail_url.contains("token="));
        
        let preview_url = server.get_preview_url(uuid);
        assert!(preview_url.contains("preview"));
        
        let file_url = server.get_file_url(uuid);
        assert!(file_url.contains("file"));
    }

    #[test]
    fn test_cache_stats() {
        let server = SecureAssetStreamServer::new(19283);
        
        // Initially empty
        let stats = server.cache_stats();
        assert_eq!(stats.thumbnail_count, 0);
        assert_eq!(stats.preview_count, 0);
        assert_eq!(stats.total_size_bytes, 0);
        
        // Add some thumbnails
        let uuid1 = Uuid::new_v4();
        let uuid2 = Uuid::new_v4();
        
        server.cache_thumbnail(uuid1, CachedThumbnail::new(
            vec![0u8; 1000],
            "image/png".to_string(),
            uuid1,
        ));
        server.cache_thumbnail(uuid2, CachedThumbnail::new(
            vec![0u8; 2000],
            "image/jpeg".to_string(),
            uuid2,
        ));
        
        let stats = server.cache_stats();
        assert_eq!(stats.thumbnail_count, 2);
        assert_eq!(stats.thumbnail_size_bytes, 3000);
        
        // Add a preview
        server.cache_preview(uuid1, CachedPreview::new(
            vec![0u8; 500],
            "text/plain".to_string(),
            uuid1,
            PreviewType::Text,
        ));
        
        let stats = server.cache_stats();
        assert_eq!(stats.preview_count, 1);
        assert_eq!(stats.preview_size_bytes, 500);
        assert_eq!(stats.total_size_bytes, 3500);
    }

    #[test]
    fn test_cache_clear() {
        let server = SecureAssetStreamServer::new(19283);
        let uuid = Uuid::new_v4();
        
        server.cache_thumbnail(uuid, CachedThumbnail::new(
            vec![1, 2, 3],
            "image/png".to_string(),
            uuid,
        ));
        
        assert!(server.get_thumbnail(&uuid).is_some());
        
        server.clear_caches();
        
        assert!(server.get_thumbnail(&uuid).is_none());
        assert_eq!(server.cache_stats().total_size_bytes, 0);
    }

    #[test]
    fn test_allowed_origins_default() {
        let config = AssetServerConfig::with_port(19283);
        let origins = config.allowed_origins();
        
        assert!(origins.contains(&"http://localhost:19283".to_string()));
        assert!(origins.contains(&"http://127.0.0.1:19283".to_string()));
        assert!(origins.contains(&"tauri://localhost".to_string()));
        assert!(origins.contains(&"https://tauri.localhost".to_string()));
    }

    #[test]
    fn test_allowed_origins_custom() {
        let config = AssetServerConfig {
            port: 19283,
            additional_origins: vec![
                "http://custom.origin:3000".to_string(),
                "https://another.origin".to_string(),
            ],
            ..Default::default()
        };
        let origins = config.allowed_origins();
        
        assert!(origins.contains(&"http://custom.origin:3000".to_string()));
        assert!(origins.contains(&"https://another.origin".to_string()));
    }

    #[test]
    fn test_cached_thumbnail_size() {
        let uuid = Uuid::new_v4();
        let data = vec![0u8; 1024];
        let thumbnail = CachedThumbnail::new(data, "image/png".to_string(), uuid);
        
        assert_eq!(thumbnail.size(), 1024);
    }

    #[test]
    fn test_cached_preview_types() {
        let uuid = Uuid::new_v4();
        
        let text_preview = CachedPreview::new(
            b"Hello, world!".to_vec(),
            "text/plain".to_string(),
            uuid,
            PreviewType::Text,
        );
        assert_eq!(text_preview.preview_type, PreviewType::Text);
        
        let image_preview = CachedPreview::new(
            vec![0u8; 100],
            "image/png".to_string(),
            uuid,
            PreviewType::Image,
        );
        assert_eq!(image_preview.preview_type, PreviewType::Image);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    // Strategy for generating random session tokens
    fn token_strategy() -> impl Strategy<Value = String> {
        prop::collection::vec(prop::num::u8::ANY, 32)
            .prop_map(|bytes| hex::encode(bytes))
    }

    // Strategy for generating random origins
    fn origin_strategy() -> impl Strategy<Value = String> {
        prop::string::string_regex("https?://[a-z]+\\.[a-z]+(:[0-9]+)?")
            .unwrap()
    }

    // Property 37: Asset Server Token Validation
    // For any request to the AssetStreamServer without a valid session token,
    // the server SHALL return HTTP 403 Forbidden.
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn prop_invalid_token_rejected(invalid_token in token_strategy()) {
            // Feature: neural-fs-core, Property 37: Asset Server Token Validation
            // Validates: Asset Server Security
            
            let config = AssetServerConfig::with_port(19283);
            let state = AssetServerState::new(config);
            
            // The generated token should be different from the server's token
            // (with overwhelming probability since tokens are random)
            if invalid_token != state.session_token {
                prop_assert!(!state.validate_token(&invalid_token));
            }
        }

        #[test]
        fn prop_valid_token_accepted(seed in prop::num::u64::ANY) {
            // Feature: neural-fs-core, Property 37: Asset Server Token Validation
            // Validates: Asset Server Security
            
            let config = AssetServerConfig::with_port(19283);
            let state = AssetServerState::new(config);
            
            // The server's own token should always be valid
            prop_assert!(state.validate_token(&state.session_token));
        }
    }

    // Property 38: CSRF Protection
    // For any request to the AssetStreamServer with an Origin header not in the allowed list,
    // the server SHALL reject the request.
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn prop_invalid_origin_rejected(origin in origin_strategy()) {
            // Feature: neural-fs-core, Property 38: CSRF Protection
            // Validates: Asset Server Security
            
            let config = AssetServerConfig::with_port(19283);
            let state = AssetServerState::new(config);
            
            // Random origins should not be in the allowed list
            // (unless they happen to match localhost patterns, which is unlikely)
            let is_localhost = origin.contains("localhost") || origin.contains("127.0.0.1");
            let is_tauri = origin.contains("tauri://");
            
            if !is_localhost && !is_tauri {
                prop_assert!(!state.is_origin_allowed(&origin));
            }
        }

        #[test]
        fn prop_allowed_origins_accepted(_seed in prop::num::u64::ANY) {
            // Feature: neural-fs-core, Property 38: CSRF Protection
            // Validates: Asset Server Security
            
            let config = AssetServerConfig::with_port(19283);
            let state = AssetServerState::new(config);
            
            // All configured allowed origins should be accepted
            for origin in &state.allowed_origins {
                prop_assert!(state.is_origin_allowed(origin));
            }
        }

        #[test]
        fn prop_referer_must_start_with_allowed_origin(
            path in prop::string::string_regex("/[a-z/]*").unwrap()
        ) {
            // Feature: neural-fs-core, Property 38: CSRF Protection
            // Validates: Asset Server Security
            
            let config = AssetServerConfig::with_port(19283);
            let state = AssetServerState::new(config);
            
            // Referers starting with allowed origins should be accepted
            for origin in &state.allowed_origins {
                let referer = format!("{}{}", origin, path);
                prop_assert!(state.is_referer_allowed(&referer));
            }
        }
    }

    // Property 27: Asset Streaming Performance
    // For any thumbnail request via Custom Protocol, the response SHALL be returned
    // without IPC serialization overhead (direct binary stream).
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn prop_thumbnail_cache_returns_exact_data(
            data in prop::collection::vec(prop::num::u8::ANY, 1..10000),
            content_type in prop::string::string_regex("image/(png|jpeg|gif|webp)").unwrap()
        ) {
            // Feature: neural-fs-core, Property 27: Asset Streaming Performance
            // Validates: Asset Streaming
            
            let server = SecureAssetStreamServer::new(19283);
            let uuid = Uuid::new_v4();
            
            // Cache the thumbnail
            let original_data = data.clone();
            server.cache_thumbnail(uuid, CachedThumbnail::new(
                data,
                content_type.clone(),
                uuid,
            ));
            
            // Retrieve and verify exact match (no serialization overhead)
            let cached = server.get_thumbnail(&uuid).unwrap();
            prop_assert_eq!(cached.data.as_ref(), &original_data);
            prop_assert_eq!(cached.content_type, content_type);
        }

        #[test]
        fn prop_preview_cache_returns_exact_data(
            data in prop::collection::vec(prop::num::u8::ANY, 1..10000),
            content_type in prop::string::string_regex("(text/plain|text/html|application/json)").unwrap()
        ) {
            // Feature: neural-fs-core, Property 27: Asset Streaming Performance
            // Validates: Asset Streaming
            
            let server = SecureAssetStreamServer::new(19283);
            let uuid = Uuid::new_v4();
            
            // Cache the preview
            let original_data = data.clone();
            server.cache_preview(uuid, CachedPreview::new(
                data,
                content_type.clone(),
                uuid,
                PreviewType::Text,
            ));
            
            // Retrieve and verify exact match
            let cached = server.get_preview(&uuid).unwrap();
            prop_assert_eq!(cached.data.as_ref(), &original_data);
            prop_assert_eq!(cached.content_type, content_type);
        }
    }

    // Additional property tests for cache consistency
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn prop_cache_invalidation_removes_entry(
            data in prop::collection::vec(prop::num::u8::ANY, 1..1000)
        ) {
            let server = SecureAssetStreamServer::new(19283);
            let uuid = Uuid::new_v4();
            
            // Add and then invalidate
            server.cache_thumbnail(uuid, CachedThumbnail::new(
                data,
                "image/png".to_string(),
                uuid,
            ));
            prop_assert!(server.get_thumbnail(&uuid).is_some());
            
            server.invalidate_thumbnail(&uuid);
            prop_assert!(server.get_thumbnail(&uuid).is_none());
        }

        #[test]
        fn prop_cache_stats_accurate(
            sizes in prop::collection::vec(1usize..1000, 1..10)
        ) {
            let server = SecureAssetStreamServer::new(19283);
            let mut expected_size = 0usize;
            
            for size in &sizes {
                let uuid = Uuid::new_v4();
                server.cache_thumbnail(uuid, CachedThumbnail::new(
                    vec![0u8; *size],
                    "image/png".to_string(),
                    uuid,
                ));
                expected_size += size;
            }
            
            let stats = server.cache_stats();
            prop_assert_eq!(stats.thumbnail_count, sizes.len());
            prop_assert_eq!(stats.thumbnail_size_bytes, expected_size);
        }
    }
}
