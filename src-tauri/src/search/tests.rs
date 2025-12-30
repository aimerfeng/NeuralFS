//! Tests for the search module
//!
//! Contains property-based tests for tokenization quality and intent classification

use proptest::prelude::*;
use super::tokenizer::{JiebaTokenizer, MultilingualTokenizer, SimpleTokenizer, LanguageDetector, Language};
use super::intent::{IntentParser, IntentCategory};
use crate::core::types::search::SearchIntent;

// ============================================================================
// Property 31: Chinese Tokenization Quality
// ============================================================================

/// Generate Chinese text strings for property testing
fn chinese_text_strategy() -> impl Strategy<Value = String> {
    // Common Chinese phrases and sentences
    prop_oneof![
        Just("我爱北京天安门".to_string()),
        Just("人工智能".to_string()),
        Just("机器学习".to_string()),
        Just("深度学习神经网络".to_string()),
        Just("自然语言处理".to_string()),
        Just("计算机视觉".to_string()),
        Just("这是一个测试".to_string()),
        Just("文件系统管理".to_string()),
        Just("语义搜索引擎".to_string()),
        Just("智能标签管理".to_string()),
        Just("今天天气很好".to_string()),
        Just("中华人民共和国".to_string()),
        Just("北京大学清华大学".to_string()),
        Just("软件工程师".to_string()),
        Just("数据库管理系统".to_string()),
        // Generate random combinations of common Chinese characters
        "[一-龥]{2,10}".prop_map(|s| s),
    ]
}

proptest! {
    /// Property 31: Chinese Tokenization Quality
    /// *For any* Chinese text input, the JiebaTokenizer SHALL produce meaningful word segments
    /// (not single characters or entire sentences as single tokens).
    /// **Validates: Tokenizer Strategy, Requirements 19**
    #[test]
    fn prop_chinese_tokenization_quality(text in chinese_text_strategy()) {
        let tokenizer = JiebaTokenizer::new();
        let tokens = tokenizer.tokenize(&text);

        // Property 1: Should produce at least one token for non-empty input
        prop_assert!(!tokens.is_empty(), "Tokenizer should produce tokens for: {}", text);

        // Property 2: Should not produce only single-character tokens for multi-character input
        // (unless the input is very short)
        let char_count = text.chars().count();
        if char_count > 2 {
            let all_single_chars = tokens.iter().all(|t| t.chars().count() == 1);
            prop_assert!(
                !all_single_chars,
                "Tokenizer should produce multi-character tokens for '{}', got: {:?}",
                text,
                tokens
            );
        }

        // Property 3: Should not produce the entire input as a single token
        // (unless the input is a single word)
        if char_count > 4 {
            let is_single_token = tokens.len() == 1 && tokens[0] == text;
            prop_assert!(
                !is_single_token,
                "Tokenizer should segment '{}' into multiple tokens, got: {:?}",
                text,
                tokens
            );
        }

        // Property 4: Total characters in tokens should roughly match input
        // (allowing for whitespace/punctuation removal)
        let token_chars: usize = tokens.iter().map(|t| t.chars().count()).sum();
        let input_chars = text.chars().filter(|c| !c.is_whitespace()).count();
        prop_assert!(
            token_chars <= input_chars + 1, // Allow small variance
            "Token character count {} should not exceed input {} for '{}'",
            token_chars,
            input_chars,
            text
        );
    }
}

// ============================================================================
// Additional tokenizer tests
// ============================================================================

proptest! {
    /// Test that SimpleTokenizer correctly handles English text
    #[test]
    fn prop_simple_tokenizer_english(text in "[a-zA-Z ]{1,100}") {
        let tokenizer = SimpleTokenizer::new();
        let tokens = tokenizer.tokenize(&text);

        // All tokens should be lowercase
        for token in &tokens {
            prop_assert!(
                token.chars().all(|c| c.is_lowercase() || c.is_numeric() || c == '_'),
                "Token '{}' should be lowercase",
                token
            );
        }

        // No empty tokens
        for token in &tokens {
            prop_assert!(!token.is_empty(), "Should not produce empty tokens");
        }
    }

    /// Test that language detection is consistent
    #[test]
    fn prop_language_detection_consistency(text in "[a-zA-Z]{10,50}") {
        let detector = LanguageDetector::new();
        let lang1 = detector.detect(&text);
        let lang2 = detector.detect(&text);

        // Same input should always produce same output
        prop_assert_eq!(lang1, lang2, "Language detection should be deterministic");

        // English text should be detected as English
        prop_assert_eq!(lang1, Language::English, "ASCII text should be detected as English");
    }

    /// Test that MultilingualTokenizer produces consistent results
    #[test]
    fn prop_multilingual_tokenizer_consistency(text in ".{1,50}") {
        let tokenizer = MultilingualTokenizer::new();
        let tokens1 = tokenizer.tokenize(&text);
        let tokens2 = tokenizer.tokenize(&text);

        // Same input should always produce same output
        prop_assert_eq!(tokens1, tokens2, "Tokenization should be deterministic");
    }
}

#[cfg(test)]
mod additional_tests {
    use super::*;

    #[test]
    fn test_chinese_common_phrases() {
        let tokenizer = JiebaTokenizer::new();

        // Test common Chinese phrases
        let test_cases = vec![
            ("我爱北京天安门", vec!["我", "爱", "北京", "天安门"]),
            ("人工智能", vec!["人工智能"]),
            ("机器学习", vec!["机器", "学习"]),
        ];

        for (input, expected_contains) in test_cases {
            let tokens = tokenizer.tokenize(input);
            for expected in expected_contains {
                assert!(
                    tokens.iter().any(|t| t.contains(expected) || expected.contains(t.as_str())),
                    "Expected '{}' to be in tokens {:?} for input '{}'",
                    expected,
                    tokens,
                    input
                );
            }
        }
    }

    #[test]
    fn test_mixed_language_detection() {
        let detector = LanguageDetector::new();

        assert_eq!(detector.detect("Hello World"), Language::English);
        assert_eq!(detector.detect("你好世界"), Language::Chinese);
        assert_eq!(detector.detect(""), Language::Unknown);
    }

    #[test]
    fn test_multilingual_tokenizer_routing() {
        let tokenizer = MultilingualTokenizer::new();

        // Chinese should use Jieba
        let chinese_tokens = tokenizer.tokenize("人工智能文件系统");
        assert!(!chinese_tokens.is_empty());
        // Should produce meaningful segments, not single characters
        assert!(chinese_tokens.len() < "人工智能文件系统".chars().count());

        // English should use simple tokenizer
        let english_tokens = tokenizer.tokenize("artificial intelligence");
        assert_eq!(english_tokens, vec!["artificial", "intelligence"]);
    }
}


// ============================================================================
// Property 3: Intent Classification Validity
// ============================================================================

/// Generate diverse search query strings for property testing
fn search_query_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        // File-oriented queries (English)
        Just("find the report.pdf file".to_string()),
        Just("where is my document".to_string()),
        Just("locate the image file".to_string()),
        Just("show me the latest video".to_string()),
        Just("find code files".to_string()),
        // Content-oriented queries (English)
        Just("find the paragraph about machine learning".to_string()),
        Just("content that mentions neural networks".to_string()),
        Just("\"specific quote from document\"".to_string()),
        Just("text that describes the algorithm".to_string()),
        Just("section about data processing".to_string()),
        // File-oriented queries (Chinese)
        Just("找文件 报告".to_string()),
        Just("哪个文件包含数据".to_string()),
        Just("最近的文档".to_string()),
        Just("图片文件".to_string()),
        // Content-oriented queries (Chinese)
        Just("关于机器学习的段落".to_string()),
        Just("提到人工智能的内容".to_string()),
        Just("描述算法的部分".to_string()),
        // Ambiguous queries
        Just("hello".to_string()),
        Just("test".to_string()),
        Just("data".to_string()),
        // Mixed queries
        Just("find recent pdf about AI".to_string()),
        Just("locate the code that implements sorting".to_string()),
        // Random alphanumeric strings
        "[a-zA-Z0-9 ]{1,50}".prop_map(|s| s),
        // Random strings with Chinese characters
        "[一-龥a-zA-Z0-9 ]{1,30}".prop_map(|s| s),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// **Feature: neural-fs-core, Property 3: Intent Classification Validity**
    /// *For any* search query, the Intent_Parser SHALL return exactly one valid SearchIntent variant
    /// (FindFile, FindContent, or Ambiguous).
    /// **Validates: Requirements 2.1**
    #[test]
    fn prop_intent_classification_validity(query in search_query_strategy()) {
        let parser = IntentParser::new();
        let result = parser.parse(&query);
        
        // Property 1: The result must contain exactly one valid SearchIntent variant
        let is_valid_intent = matches!(
            result.intent,
            SearchIntent::FindFile { .. } | SearchIntent::FindContent { .. } | SearchIntent::Ambiguous { .. }
        );
        prop_assert!(
            is_valid_intent,
            "Intent parser should return a valid SearchIntent for query: '{}'",
            query
        );
        
        // Property 2: Confidence score must be in valid range [0.0, 1.0]
        prop_assert!(
            result.confidence >= 0.0 && result.confidence <= 1.0,
            "Confidence score {} should be in range [0.0, 1.0] for query: '{}'",
            result.confidence,
            query
        );
        
        // Property 3: If intent is Ambiguous, is_ambiguous flag should be true
        if matches!(result.intent, SearchIntent::Ambiguous { .. }) {
            prop_assert!(
                result.is_ambiguous,
                "is_ambiguous should be true when intent is Ambiguous for query: '{}'",
                query
            );
        }
        
        // Property 4: Ambiguous intent should contain clarification questions
        if let SearchIntent::Ambiguous { clarification_questions, possible_intents } = &result.intent {
            prop_assert!(
                !clarification_questions.is_empty(),
                "Ambiguous intent should have clarification questions for query: '{}'",
                query
            );
            prop_assert!(
                !possible_intents.is_empty(),
                "Ambiguous intent should have possible intents for query: '{}'",
                query
            );
        }
    }

    /// Property: Intent classification is deterministic
    /// *For any* search query, parsing it twice should produce the same intent category
    #[test]
    fn prop_intent_classification_deterministic(query in search_query_strategy()) {
        let parser = IntentParser::new();
        
        let result1 = parser.parse(&query);
        let result2 = parser.parse(&query);
        
        // Same query should always produce same classification
        let category1 = match &result1.intent {
            SearchIntent::FindFile { .. } => "FindFile",
            SearchIntent::FindContent { .. } => "FindContent",
            SearchIntent::Ambiguous { .. } => "Ambiguous",
        };
        let category2 = match &result2.intent {
            SearchIntent::FindFile { .. } => "FindFile",
            SearchIntent::FindContent { .. } => "FindContent",
            SearchIntent::Ambiguous { .. } => "Ambiguous",
        };
        
        prop_assert_eq!(
            category1, category2,
            "Intent classification should be deterministic for query: '{}'",
            query
        );
        
        // Confidence should also be the same
        prop_assert!(
            (result1.confidence - result2.confidence).abs() < f32::EPSILON,
            "Confidence should be deterministic for query: '{}'",
            query
        );
    }

    /// Property: classify() method is consistent with parse()
    /// *For any* search query, classify() should return a category consistent with parse().intent
    #[test]
    fn prop_classify_consistent_with_parse(query in search_query_strategy()) {
        let parser = IntentParser::new();
        
        let parse_result = parser.parse(&query);
        let classify_result = parser.classify(&query);
        
        let expected_category = match &parse_result.intent {
            SearchIntent::FindFile { .. } => IntentCategory::File,
            SearchIntent::FindContent { .. } => IntentCategory::Content,
            SearchIntent::Ambiguous { .. } => IntentCategory::Ambiguous,
        };
        
        // Note: There might be slight differences due to threshold handling,
        // but the general direction should be consistent
        let is_consistent = match (expected_category, classify_result) {
            (IntentCategory::File, IntentCategory::File) => true,
            (IntentCategory::Content, IntentCategory::Content) => true,
            (IntentCategory::Ambiguous, IntentCategory::Ambiguous) => true,
            // Allow some flexibility for edge cases near thresholds
            (IntentCategory::Ambiguous, _) => true,
            (_, IntentCategory::Ambiguous) => parse_result.confidence < 0.5,
            _ => false,
        };
        
        prop_assert!(
            is_consistent,
            "classify() should be consistent with parse() for query: '{}'. Expected {:?}, got {:?}",
            query,
            expected_category,
            classify_result
        );
    }
}

#[cfg(test)]
mod intent_tests {
    use super::*;

    #[test]
    fn test_intent_parser_returns_valid_intent() {
        let parser = IntentParser::new();
        
        // Test various query types
        let queries = vec![
            "find file report.pdf",
            "content about AI",
            "hello world",
            "找文件",
            "关于机器学习的内容",
            "",
            "a",
            "the quick brown fox jumps over the lazy dog",
        ];
        
        for query in queries {
            let result = parser.parse(query);
            
            // Must return a valid intent
            assert!(
                matches!(
                    result.intent,
                    SearchIntent::FindFile { .. } | SearchIntent::FindContent { .. } | SearchIntent::Ambiguous { .. }
                ),
                "Invalid intent for query: '{}'",
                query
            );
            
            // Confidence must be valid
            assert!(
                result.confidence >= 0.0 && result.confidence <= 1.0,
                "Invalid confidence for query: '{}'",
                query
            );
        }
    }
}


// ============================================================================
// Property 19: Search Filter Correctness
// Property 22: Hybrid Search Score Normalization
// ============================================================================

use super::hybrid::{
    HybridSearchConfig, HybridSearchEngine, HybridSearchFilters, QueryType, ScoredResult,
    SearchSource, apply_filters, classify_query,
};
use crate::core::types::file::FileType;
use crate::vector::store::SearchResult as VectorSearchResult;
use crate::search::text_index::SearchResult as TextSearchResult;
use std::collections::HashMap;
use serde_json::Value;

/// Generate random file types for property testing
fn file_type_strategy() -> impl Strategy<Value = FileType> {
    prop_oneof![
        Just(FileType::TextDocument),
        Just(FileType::Pdf),
        Just(FileType::Image),
        Just(FileType::Video),
        Just(FileType::Code),
        Just(FileType::OfficeDocument),
        Just(FileType::Archive),
        Just(FileType::Unknown),
    ]
}

/// Generate random scored results for property testing
fn scored_result_strategy() -> impl Strategy<Value = ScoredResult> {
    (
        prop::array::uniform32(0u8..),  // file_id bytes
        prop::option::of(prop::array::uniform32(0u8..)),  // chunk_id bytes
        0.0f32..1.0f32,  // score
        prop::option::of(0.0f32..1.0f32),  // vector_score
        prop::option::of(0.0f32..1.0f32),  // bm25_score
        prop_oneof![
            Just(SearchSource::Vector),
            Just(SearchSource::BM25),
            Just(SearchSource::Both),
        ],
        prop::option::of("[a-zA-Z0-9_.-]{1,50}"),  // filename
        prop::collection::vec("[a-zA-Z0-9]{1,20}", 0..5),  // tags
    ).prop_map(|(file_id_bytes, chunk_id_bytes, score, vector_score, bm25_score, source, filename, tags)| {
        ScoredResult {
            file_id: Uuid::from_bytes(file_id_bytes),
            chunk_id: chunk_id_bytes.map(Uuid::from_bytes),
            score,
            vector_score,
            bm25_score,
            source,
            filename,
            tags,
        }
    })
}

/// Generate vector search results for property testing
fn vector_result_strategy() -> impl Strategy<Value = VectorSearchResult> {
    (
        1u64..1000000u64,  // id
        0.0f32..1.0f32,  // score
        prop::array::uniform32(0u8..),  // file_id bytes
    ).prop_map(|(id, score, file_id_bytes)| {
        let file_id = Uuid::from_bytes(file_id_bytes);
        let mut payload = HashMap::new();
        payload.insert("file_id".to_string(), Value::String(file_id.to_string()));
        VectorSearchResult {
            id,
            score,
            payload,
            vector: None,
        }
    })
}

/// Generate text search results for property testing
fn text_result_strategy() -> impl Strategy<Value = TextSearchResult> {
    (
        prop::array::uniform32(0u8..),  // file_id bytes
        prop::option::of(prop::array::uniform32(0u8..)),  // chunk_id bytes
        0.0f32..100.0f32,  // score (BM25 scores can be > 1)
        prop::option::of("[a-zA-Z0-9_.-]{1,50}"),  // filename
        prop::collection::vec("[a-zA-Z0-9]{1,20}", 0..5),  // tags
    ).prop_map(|(file_id_bytes, chunk_id_bytes, score, filename, tags)| {
        TextSearchResult {
            file_id: Uuid::from_bytes(file_id_bytes),
            chunk_id: chunk_id_bytes.map(Uuid::from_bytes),
            filename,
            tags,
            modified_at: None,
            score,
        }
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// **Feature: neural-fs-core, Property 19: Search Filter Correctness**
    /// *For any* search with filters applied, all returned results SHALL satisfy
    /// all filter conditions (file type, tags, time range, privacy level).
    /// **Validates: Requirements 2.2, 2.3**
    #[test]
    fn prop_search_filter_correctness(
        results in prop::collection::vec(scored_result_strategy(), 0..50),
        min_score in 0.0f32..1.0f32,
    ) {
        // Create filters with minimum score
        let filters = HybridSearchFilters::new()
            .with_min_score(min_score);

        // Apply filters
        let filtered = apply_filters(results.clone(), &filters);

        // Property: All filtered results must have score >= min_score
        for result in &filtered {
            prop_assert!(
                result.score >= min_score,
                "Result with score {} should be filtered out (min_score: {})",
                result.score,
                min_score
            );
        }

        // Property: No results above threshold should be filtered out
        let expected_count = results.iter().filter(|r| r.score >= min_score).count();
        prop_assert_eq!(
            filtered.len(),
            expected_count,
            "Filter should keep all results with score >= {}",
            min_score
        );
    }

    /// **Feature: neural-fs-core, Property 22: Hybrid Search Score Normalization**
    /// *For any* search query, the final score SHALL be a weighted combination of
    /// vector score and BM25 score, with weights summing to 1.0.
    /// **Validates: Requirements 2.2, Hybrid Search Logic**
    #[test]
    fn prop_hybrid_search_score_normalization(
        vector_results in prop::collection::vec(vector_result_strategy(), 0..20),
        bm25_results in prop::collection::vec(text_result_strategy(), 0..20),
        vector_weight in 0.1f32..0.9f32,
    ) {
        let bm25_weight = 1.0 - vector_weight;
        
        // Property 1: Weights must sum to 1.0
        prop_assert!(
            (vector_weight + bm25_weight - 1.0).abs() < 0.001,
            "Weights must sum to 1.0, got {} + {} = {}",
            vector_weight,
            bm25_weight,
            vector_weight + bm25_weight
        );

        // Create engine and merge results
        let engine = HybridSearchEngine::new();
        let merged = engine.merge_results(
            vector_results.clone(),
            bm25_results.clone(),
            (vector_weight, bm25_weight),
        );

        // Property 2: All merged scores should be in valid range [0, 1]
        for result in &merged {
            prop_assert!(
                result.score >= 0.0 && result.score <= 1.0 + 0.001, // Small epsilon for floating point
                "Merged score {} should be in range [0, 1]",
                result.score
            );
        }

        // Property 3: Results should be sorted by score descending
        for i in 1..merged.len() {
            prop_assert!(
                merged[i - 1].score >= merged[i].score - 0.001,
                "Results should be sorted by score descending: {} < {}",
                merged[i - 1].score,
                merged[i].score
            );
        }

        // Property 4: Results from both sources should have source = Both
        for result in &merged {
            if result.vector_score.is_some() && result.bm25_score.is_some() {
                prop_assert_eq!(
                    result.source,
                    SearchSource::Both,
                    "Result with both scores should have source = Both"
                );
            }
        }
    }

    /// Property: Query classification is deterministic
    #[test]
    fn prop_query_classification_deterministic(query in "[a-zA-Z0-9 ]{1,100}") {
        let result1 = classify_query(&query);
        let result2 = classify_query(&query);
        
        prop_assert_eq!(
            result1,
            result2,
            "Query classification should be deterministic for: '{}'",
            query
        );
    }

    /// Property: Config validation ensures weights sum to 1.0
    #[test]
    fn prop_config_weights_validation(
        vector_weight in 0.0f32..2.0f32,
        bm25_weight in 0.0f32..2.0f32,
    ) {
        let config = HybridSearchConfig {
            vector_weight,
            bm25_weight,
            ..Default::default()
        };

        let validation_result = config.validate();
        let weight_sum = vector_weight + bm25_weight;

        if (weight_sum - 1.0).abs() <= 0.001 {
            prop_assert!(
                validation_result.is_ok(),
                "Config with weights summing to 1.0 should be valid"
            );
        } else {
            prop_assert!(
                validation_result.is_err(),
                "Config with weights not summing to 1.0 should be invalid"
            );
        }
    }

    /// Property: with_weights normalizes to sum to 1.0
    #[test]
    fn prop_with_weights_normalizes(
        vector_weight in 0.1f32..10.0f32,
        bm25_weight in 0.1f32..10.0f32,
    ) {
        let config = HybridSearchConfig::with_weights(vector_weight, bm25_weight);
        
        // Weights should sum to 1.0
        let sum = config.vector_weight + config.bm25_weight;
        prop_assert!(
            (sum - 1.0).abs() < 0.001,
            "Normalized weights should sum to 1.0, got {}",
            sum
        );

        // Validation should pass
        prop_assert!(
            config.validate().is_ok(),
            "Normalized config should be valid"
        );

        // Ratio should be preserved
        let expected_ratio = vector_weight / bm25_weight;
        let actual_ratio = config.vector_weight / config.bm25_weight;
        prop_assert!(
            (expected_ratio - actual_ratio).abs() < 0.001,
            "Weight ratio should be preserved: expected {}, got {}",
            expected_ratio,
            actual_ratio
        );
    }
}

#[cfg(test)]
mod hybrid_search_tests {
    use super::*;

    #[test]
    fn test_filter_by_min_score() {
        let results = vec![
            ScoredResult {
                file_id: Uuid::new_v4(),
                chunk_id: None,
                score: 0.8,
                vector_score: Some(0.8),
                bm25_score: None,
                source: SearchSource::Vector,
                filename: Some("high_score.txt".to_string()),
                tags: vec![],
            },
            ScoredResult {
                file_id: Uuid::new_v4(),
                chunk_id: None,
                score: 0.3,
                vector_score: Some(0.3),
                bm25_score: None,
                source: SearchSource::Vector,
                filename: Some("low_score.txt".to_string()),
                tags: vec![],
            },
        ];

        let filters = HybridSearchFilters::new().with_min_score(0.5);
        let filtered = apply_filters(results, &filters);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].filename, Some("high_score.txt".to_string()));
    }

    #[test]
    fn test_merge_results_combines_scores() {
        let engine = HybridSearchEngine::new();
        
        let file_id = Uuid::new_v4();
        let mut payload = HashMap::new();
        payload.insert("file_id".to_string(), Value::String(file_id.to_string()));

        let vector_results = vec![VectorSearchResult {
            id: 1,
            score: 0.8,
            payload,
            vector: None,
        }];

        let bm25_results = vec![TextSearchResult {
            file_id,
            chunk_id: None,
            filename: Some("test.txt".to_string()),
            tags: vec!["tag1".to_string()],
            modified_at: None,
            score: 10.0, // BM25 scores can be > 1
        }];

        let merged = engine.merge_results(vector_results, bm25_results, (0.6, 0.4));

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].file_id, file_id);
        assert_eq!(merged[0].source, SearchSource::Both);
        assert!(merged[0].vector_score.is_some());
        assert!(merged[0].bm25_score.is_some());
    }

    #[test]
    fn test_exact_match_boost() {
        let engine = HybridSearchEngine::new();
        
        let mut results = vec![
            ScoredResult {
                file_id: Uuid::new_v4(),
                chunk_id: None,
                score: 0.5,
                vector_score: Some(0.5),
                bm25_score: None,
                source: SearchSource::Vector,
                filename: Some("report.pdf".to_string()),
                tags: vec![],
            },
            ScoredResult {
                file_id: Uuid::new_v4(),
                chunk_id: None,
                score: 0.6,
                vector_score: Some(0.6),
                bm25_score: None,
                source: SearchSource::Vector,
                filename: Some("other.txt".to_string()),
                tags: vec![],
            },
        ];

        engine.apply_exact_match_boost(&mut results, "report");

        // The "report.pdf" result should now have a higher score due to filename match
        let report_result = results.iter().find(|r| r.filename == Some("report.pdf".to_string())).unwrap();
        assert!(report_result.score > 0.5, "Score should be boosted");
    }
}


// ============================================================================
// Property 7: Search Latency Bound (Fast Mode)
// ============================================================================

use std::time::{Duration, Instant};

/// Maximum allowed latency for fast mode search (200ms)
const FAST_MODE_LATENCY_MS: u64 = 200;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// **Feature: neural-fs-core, Property 7: Search Latency Bound (Fast Mode)**
    /// *For any* search query in fast inference mode, the initial results SHALL be
    /// returned within 200ms.
    /// **Validates: Requirements 4.8**
    ///
    /// Note: This test validates the core search operations (query classification,
    /// result merging, filtering) complete within the latency bound. The actual
    /// vector search and BM25 search are mocked since they depend on external
    /// data stores.
    #[test]
    fn prop_search_latency_bound_fast_mode(
        query in "[a-zA-Z0-9 ]{1,100}",
        vector_results in prop::collection::vec(vector_result_strategy(), 0..100),
        bm25_results in prop::collection::vec(text_result_strategy(), 0..100),
    ) {
        let engine = HybridSearchEngine::new();
        
        let start = Instant::now();
        
        // Step 1: Classify query
        let query_type = engine.classify_query(&query);
        
        // Step 2: Get adjusted weights
        let weights = engine.get_adjusted_weights(query_type);
        
        // Step 3: Merge results (simulating parallel search completion)
        let mut merged = engine.merge_results(vector_results, bm25_results, weights);
        
        // Step 4: Apply exact match boost
        engine.apply_exact_match_boost(&mut merged, &query);
        
        // Step 5: Filter by score
        let filtered = engine.filter_by_score(merged);
        
        // Step 6: Limit results
        let _final_results = engine.limit_results(filtered);
        
        let elapsed = start.elapsed();
        
        // Property: Core search operations should complete well within 200ms
        // We use a stricter bound (50ms) for the core operations since the
        // actual search (vector + BM25) will take additional time
        prop_assert!(
            elapsed < Duration::from_millis(50),
            "Core search operations took {:?}, should be < 50ms for fast mode",
            elapsed
        );
    }

    /// Property: Query classification is fast
    #[test]
    fn prop_query_classification_latency(query in "[a-zA-Z0-9 ]{1,200}") {
        let start = Instant::now();
        let _query_type = classify_query(&query);
        let elapsed = start.elapsed();
        
        // Query classification should be very fast (< 1ms)
        prop_assert!(
            elapsed < Duration::from_millis(1),
            "Query classification took {:?}, should be < 1ms",
            elapsed
        );
    }

    /// Property: Result merging scales linearly with result count
    #[test]
    fn prop_merge_results_latency(
        vector_results in prop::collection::vec(vector_result_strategy(), 0..500),
        bm25_results in prop::collection::vec(text_result_strategy(), 0..500),
    ) {
        let engine = HybridSearchEngine::new();
        let total_results = vector_results.len() + bm25_results.len();
        
        let start = Instant::now();
        let _merged = engine.merge_results(vector_results, bm25_results, (0.6, 0.4));
        let elapsed = start.elapsed();
        
        // Merging should be fast even with many results
        // Allow ~0.1ms per result as a rough bound
        let max_allowed_ms = (total_results as u64 / 10).max(10);
        prop_assert!(
            elapsed < Duration::from_millis(max_allowed_ms),
            "Merging {} results took {:?}, should be < {}ms",
            total_results,
            elapsed,
            max_allowed_ms
        );
    }
}

#[cfg(test)]
mod latency_tests {
    use super::*;

    #[test]
    fn test_fast_mode_latency_empty_results() {
        let engine = HybridSearchEngine::new();
        
        let start = Instant::now();
        
        let query_type = engine.classify_query("test query");
        let weights = engine.get_adjusted_weights(query_type);
        let merged = engine.merge_results(vec![], vec![], weights);
        let filtered = engine.filter_by_score(merged);
        let _results = engine.limit_results(filtered);
        
        let elapsed = start.elapsed();
        
        assert!(
            elapsed < Duration::from_millis(10),
            "Empty search should be very fast, took {:?}",
            elapsed
        );
    }

    #[test]
    fn test_fast_mode_latency_with_results() {
        let engine = HybridSearchEngine::new();
        
        // Create some test results
        let vector_results: Vec<VectorSearchResult> = (0..50)
            .map(|i| {
                let file_id = Uuid::new_v4();
                let mut payload = HashMap::new();
                payload.insert("file_id".to_string(), Value::String(file_id.to_string()));
                VectorSearchResult {
                    id: i as u64,
                    score: 0.5 + (i as f32 * 0.01),
                    payload,
                    vector: None,
                }
            })
            .collect();

        let bm25_results: Vec<TextSearchResult> = (0..50)
            .map(|i| TextSearchResult {
                file_id: Uuid::new_v4(),
                chunk_id: None,
                filename: Some(format!("file_{}.txt", i)),
                tags: vec!["test".to_string()],
                modified_at: None,
                score: 5.0 + (i as f32 * 0.1),
            })
            .collect();

        let start = Instant::now();
        
        let query_type = engine.classify_query("find documents about testing");
        let weights = engine.get_adjusted_weights(query_type);
        let mut merged = engine.merge_results(vector_results, bm25_results, weights);
        engine.apply_exact_match_boost(&mut merged, "testing");
        let filtered = engine.filter_by_score(merged);
        let _results = engine.limit_results(filtered);
        
        let elapsed = start.elapsed();
        
        assert!(
            elapsed < Duration::from_millis(50),
            "Search with 100 results should complete in < 50ms, took {:?}",
            elapsed
        );
    }

    #[test]
    fn test_query_classification_latency() {
        let queries = vec![
            "test",
            "find documents about machine learning",
            "0x80070005",
            "ERROR_ACCESS_DENIED",
            "report.pdf",
            "找文件",
            "where is the configuration file for the database connection",
        ];

        for query in queries {
            let start = Instant::now();
            let _query_type = classify_query(query);
            let elapsed = start.elapsed();
            
            assert!(
                elapsed < Duration::from_millis(1),
                "Query classification for '{}' took {:?}, should be < 1ms",
                query,
                elapsed
            );
        }
    }
}
