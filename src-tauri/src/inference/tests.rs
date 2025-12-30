//! Property-based tests for the inference module
//!
//! This module contains property-based tests for:
//! - Property 11: Parallel Inference Dispatch
//! - Property 12: Cache Hit Consistency
//! - Property 13: Data Anonymization
//!
//! **Validates: Requirements 11, 13**

use proptest::prelude::*;
use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;

use crate::core::types::search::SearchIntent;
use crate::embeddings::{EmbeddingConfig, EmbeddingEngine};
use crate::inference::anonymizer::{AnonymizationConfig, DataAnonymizer};
use crate::inference::cloud::{CloudConfig, CostTracker, RateLimiter};
use crate::inference::hybrid::{HybridInferenceEngine, InferenceCache};
use crate::inference::local::TagMatcher;
use crate::inference::merger::{MergerConfig, ResultMerger};
use crate::inference::types::{
    InferenceContext, InferenceOptions, InferenceRequest, InferenceResponse, InferenceSource,
};

// Strategy for generating valid queries
fn query_strategy() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9 ]{1,100}".prop_map(|s| s.trim().to_string())
        .prop_filter("non-empty query", |s| !s.is_empty())
}

// Strategy for generating email addresses
fn email_strategy() -> impl Strategy<Value = String> {
    ("[a-z]{3,10}", "[a-z]{3,10}", "[a-z]{2,4}")
        .prop_map(|(user, domain, tld)| format!("{}@{}.{}", user, domain, tld))
}

// Strategy for generating IPv4 addresses
fn ipv4_strategy() -> impl Strategy<Value = String> {
    (0u8..=255, 0u8..=255, 0u8..=255, 0u8..=255)
        .prop_map(|(a, b, c, d)| format!("{}.{}.{}.{}", a, b, c, d))
}

// Strategy for generating file paths
fn path_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        // Windows paths
        ("[A-Z]", "[a-zA-Z0-9_]{1,20}", "[a-zA-Z0-9_]{1,20}")
            .prop_map(|(drive, dir1, dir2)| format!("{}:\\Users\\{}\\{}", drive, dir1, dir2)),
        // Unix paths
        ("[a-zA-Z0-9_]{1,20}", "[a-zA-Z0-9_]{1,20}")
            .prop_map(|(user, dir)| format!("/home/{}/{}", user, dir)),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// **Feature: neural-fs-core, Property 11: Parallel Inference Dispatch**
    /// *For any* valid inference request, the hybrid engine should always return
    /// a response with at least local inference sources
    /// **Validates: Requirements 11.1**
    #[test]
    fn prop_parallel_inference_always_returns_local_sources(
        query in query_strategy()
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let embedding_engine = Arc::new(EmbeddingEngine::new(EmbeddingConfig::default()));
            let engine = HybridInferenceEngine::new(embedding_engine);
            
            let request = InferenceRequest::simple(query);
            let result = engine.infer(request).await;
            
            // Should always succeed with local inference
            prop_assert!(result.is_ok(), "Inference should succeed");
            
            let response = result.unwrap();
            
            // Should always have local embedding source
            prop_assert!(
                response.sources.contains(&InferenceSource::LocalEmbedding),
                "Response should contain LocalEmbedding source"
            );
            
            // Should always have local intent source
            prop_assert!(
                response.sources.contains(&InferenceSource::LocalIntent),
                "Response should contain LocalIntent source"
            );
            
            Ok(())
        })?;
    }

    /// **Feature: neural-fs-core, Property 12: Cache Hit Consistency**
    /// *For any* query, if we cache a response and retrieve it, the retrieved
    /// response should be equivalent to the original (with Cache source added)
    /// **Validates: Requirements 11.8**
    #[test]
    fn prop_cache_hit_consistency(
        query in query_strategy()
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let cache = InferenceCache::new(100);
            
            // Create a test response
            let original_response = InferenceResponse {
                request_id: Uuid::now_v7(),
                intent: SearchIntent::FindFile {
                    file_type_hint: None,
                    time_hint: None,
                },
                query_embedding: vec![0.1, 0.2, 0.3],
                cloud_understanding: None,
                cloud_enhanced: false,
                duration_ms: 100,
                sources: vec![InferenceSource::LocalEmbedding],
            };
            
            // Put in cache
            cache.put(&query, original_response.clone()).await;
            
            // Get from cache
            let cached = cache.get(&query).await;
            
            prop_assert!(cached.is_some(), "Cache should return the entry");
            
            let cached_response = cached.unwrap();
            
            // Core fields should match
            prop_assert_eq!(
                cached_response.request_id,
                original_response.request_id,
                "Request ID should match"
            );
            prop_assert_eq!(
                cached_response.query_embedding,
                original_response.query_embedding,
                "Query embedding should match"
            );
            prop_assert_eq!(
                cached_response.cloud_enhanced,
                original_response.cloud_enhanced,
                "Cloud enhanced flag should match"
            );
            
            // Cache source should be added
            prop_assert!(
                cached_response.sources.contains(&InferenceSource::Cache),
                "Cached response should have Cache source"
            );
            
            Ok(())
        })?;
    }

    /// **Feature: neural-fs-core, Property 13: Data Anonymization**
    /// *For any* input containing email addresses, the anonymized output
    /// should not contain the original email
    /// **Validates: Requirements 13.2**
    #[test]
    fn prop_anonymization_removes_emails(
        email in email_strategy(),
        prefix in "[a-zA-Z ]{0,20}",
        suffix in "[a-zA-Z ]{0,20}"
    ) {
        let anonymizer = DataAnonymizer::new();
        
        let input = format!("{} {} {}", prefix, email, suffix);
        let output = anonymizer.anonymize(&input);
        
        // Output should not contain the original email
        prop_assert!(
            !output.contains(&email),
            "Anonymized output should not contain email: {} in {}",
            email,
            output
        );
        
        // Output should contain the placeholder
        prop_assert!(
            output.contains("[EMAIL]"),
            "Anonymized output should contain [EMAIL] placeholder"
        );
    }

    /// **Feature: neural-fs-core, Property 13: Data Anonymization**
    /// *For any* input containing IPv4 addresses, the anonymized output
    /// should not contain the original IP
    /// **Validates: Requirements 13.2**
    #[test]
    fn prop_anonymization_removes_ipv4(
        ip in ipv4_strategy(),
        prefix in "[a-zA-Z ]{0,20}",
        suffix in "[a-zA-Z ]{0,20}"
    ) {
        let anonymizer = DataAnonymizer::new();
        
        let input = format!("{} {} {}", prefix, ip, suffix);
        let output = anonymizer.anonymize(&input);
        
        // Output should not contain the original IP
        prop_assert!(
            !output.contains(&ip),
            "Anonymized output should not contain IP: {} in {}",
            ip,
            output
        );
        
        // Output should contain the placeholder
        prop_assert!(
            output.contains("[IP]"),
            "Anonymized output should contain [IP] placeholder"
        );
    }

    /// **Feature: neural-fs-core, Property 13: Data Anonymization**
    /// *For any* input containing file paths, the anonymized output
    /// should have sensitive parts replaced
    /// **Validates: Requirements 13.2**
    #[test]
    fn prop_anonymization_handles_paths(
        path in path_strategy(),
        prefix in "[a-zA-Z ]{0,20}",
        suffix in "[a-zA-Z ]{0,20}"
    ) {
        let anonymizer = DataAnonymizer::new();
        
        let input = format!("{} {} {}", prefix, path, suffix);
        let output = anonymizer.anonymize(&input);
        
        // Output should have some anonymization applied
        // (either [USER], [HOME], or other placeholders)
        let has_placeholder = output.contains("[USER]") 
            || output.contains("[HOME]")
            || output.contains("[DOCUMENTS]")
            || output.contains("[DESKTOP]")
            || output.contains("[DOWNLOADS]");
        
        // If the path contains sensitive directories, it should be anonymized
        let has_sensitive = path.contains("Users") || path.contains("home");
        
        if has_sensitive {
            prop_assert!(
                has_placeholder || !output.contains(&path),
                "Sensitive path should be anonymized: {} -> {}",
                path,
                output
            );
        }
    }

    /// **Feature: neural-fs-core, Property 12: Cache Hit Consistency**
    /// *For any* sequence of cache operations, the cache should maintain
    /// consistency (no data corruption)
    /// **Validates: Requirements 11.8**
    #[test]
    fn prop_cache_maintains_consistency(
        queries in proptest::collection::vec(query_strategy(), 1..20)
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let cache = InferenceCache::new(10); // Small cache to force evictions
            
            let mut stored_ids: Vec<(String, Uuid)> = Vec::new();
            
            // Store responses
            for query in &queries {
                let id = Uuid::now_v7();
                let response = InferenceResponse {
                    request_id: id,
                    intent: SearchIntent::FindFile {
                        file_type_hint: None,
                        time_hint: None,
                    },
                    query_embedding: vec![],
                    cloud_understanding: None,
                    cloud_enhanced: false,
                    duration_ms: 0,
                    sources: vec![],
                };
                
                cache.put(query, response).await;
                stored_ids.push((query.clone(), id));
            }
            
            // Verify consistency: if a query is in cache, it should have correct ID
            for (query, expected_id) in &stored_ids {
                if let Some(cached) = cache.get(query).await {
                    prop_assert_eq!(
                        cached.request_id,
                        *expected_id,
                        "Cached response should have correct ID for query: {}",
                        query
                    );
                }
                // If not in cache, that's fine (eviction)
            }
            
            // Cache stats should be consistent
            let stats = cache.stats().await;
            prop_assert!(
                stats.entry_count <= stats.max_entries,
                "Entry count should not exceed max entries"
            );
            
            Ok(())
        })?;
    }

    /// **Feature: neural-fs-core, Property 11: Parallel Inference Dispatch**
    /// *For any* merger configuration with valid weights, merged scores
    /// should be within valid range [0, 1]
    /// **Validates: Requirements 11.5**
    #[test]
    fn prop_merged_scores_in_valid_range(
        local_score in 0.0f32..=1.0,
        cloud_score in proptest::option::of(0.0f32..=1.0),
        local_weight in 0.0f32..=1.0,
        cloud_weight in 0.0f32..=1.0
    ) {
        let config = MergerConfig {
            local_weight,
            cloud_weight,
            min_merge_score: 0.0,
            prefer_cloud_intent: true,
            max_results: 20,
        };
        
        let merger = ResultMerger::with_config(config);
        let merged_score = merger.merge_scores(local_score, cloud_score);
        
        prop_assert!(
            merged_score >= 0.0 && merged_score <= 1.0,
            "Merged score {} should be in [0, 1] range",
            merged_score
        );
    }

    /// **Feature: neural-fs-core, Property 13: Data Anonymization**
    /// *For any* input without sensitive data, anonymization should
    /// preserve the original content
    /// **Validates: Requirements 13.2**
    #[test]
    fn prop_anonymization_preserves_safe_content(
        safe_content in "[a-zA-Z ]{1,50}"
    ) {
        // Create anonymizer with all features enabled
        let anonymizer = DataAnonymizer::new();
        
        // Content without emails, IPs, or paths should be preserved
        let output = anonymizer.anonymize(&safe_content);
        
        // If the content doesn't contain sensitive patterns, it should be mostly preserved
        // (username replacement might still occur if it matches env vars)
        let contains_sensitive = anonymizer.contains_sensitive(&safe_content);
        
        if !contains_sensitive {
            // The output should be similar to input (allowing for username replacement)
            prop_assert!(
                output.len() >= safe_content.len() / 2,
                "Safe content should be mostly preserved: {} -> {}",
                safe_content,
                output
            );
        }
    }
}

// Additional non-property tests for edge cases

#[tokio::test]
async fn test_rate_limiter_respects_limit() {
    let limiter = RateLimiter::new(5);
    
    // Should allow 5 requests
    for _ in 0..5 {
        assert!(limiter.acquire().await.is_ok());
    }
    
    // 6th request should fail
    assert!(limiter.acquire().await.is_err());
}

#[tokio::test]
async fn test_cost_tracker_limit_enforcement() {
    let tracker = CostTracker::new(0.001); // Very low limit
    
    // Record some usage
    tracker.record(1_000_000, &crate::inference::cloud::CloudConfig::default().model).await;
    
    // Should hit limit
    assert!(tracker.is_limit_reached());
}

#[test]
fn test_tag_matcher_empty_tags() {
    let matcher = TagMatcher::new();
    let matches = matcher.match_tags("test query", &[]);
    assert!(matches.is_empty());
}

#[test]
fn test_anonymizer_disabled_features() {
    let config = AnonymizationConfig {
        anonymize_usernames: false,
        anonymize_paths: false,
        anonymize_emails: false,
        anonymize_ips: false,
        custom_patterns: Vec::new(),
        preserve_words: HashSet::new(),
    };
    
    let anonymizer = DataAnonymizer::with_config(config);
    
    let input = "test@example.com 192.168.1.1";
    let output = anonymizer.anonymize(input);
    
    // Nothing should be anonymized
    assert_eq!(input, output);
}

#[test]
fn test_merger_config_presets() {
    let local = MergerConfig::local_preferred();
    assert!(local.local_weight > local.cloud_weight);
    
    let cloud = MergerConfig::cloud_preferred();
    assert!(cloud.cloud_weight > cloud.local_weight);
    
    let balanced = MergerConfig::balanced();
    assert_eq!(balanced.local_weight, balanced.cloud_weight);
}
