//! Tests for the search module
//!
//! Contains property-based tests for tokenization quality

use proptest::prelude::*;
use super::tokenizer::{JiebaTokenizer, MultilingualTokenizer, SimpleTokenizer, LanguageDetector, Language};

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
