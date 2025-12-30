//! Multi-language tokenization support for NeuralFS
//!
//! Provides tokenizers for:
//! - Chinese (via jieba-rs)
//! - Japanese (via lindera, optional)
//! - English and other languages (simple whitespace/punctuation tokenizer)

use std::sync::Arc;
use tantivy::tokenizer::{
    BoxTokenStream, Token, TokenStream, Tokenizer as TantivyTokenizer,
};

/// Detected language for text
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Chinese,
    Japanese,
    English,
    Unknown,
}

/// Simple language detector based on character ranges
#[derive(Debug, Clone, Default)]
pub struct LanguageDetector;

impl LanguageDetector {
    pub fn new() -> Self {
        Self
    }

    /// Detect the primary language of the given text
    pub fn detect(&self, text: &str) -> Language {
        let mut chinese_count = 0;
        let mut japanese_count = 0;
        let mut latin_count = 0;
        let mut total_chars = 0;

        for ch in text.chars() {
            if ch.is_whitespace() || ch.is_ascii_punctuation() {
                continue;
            }
            total_chars += 1;

            // CJK Unified Ideographs (Chinese characters, also used in Japanese)
            if ('\u{4E00}'..='\u{9FFF}').contains(&ch) {
                chinese_count += 1;
            }
            // Hiragana
            else if ('\u{3040}'..='\u{309F}').contains(&ch) {
                japanese_count += 1;
            }
            // Katakana
            else if ('\u{30A0}'..='\u{30FF}').contains(&ch) {
                japanese_count += 1;
            }
            // Basic Latin
            else if ch.is_ascii_alphabetic() {
                latin_count += 1;
            }
        }

        if total_chars == 0 {
            return Language::Unknown;
        }

        // If there's significant Japanese-specific characters, it's Japanese
        if japanese_count > 0 && japanese_count as f32 / total_chars as f32 > 0.1 {
            return Language::Japanese;
        }

        // If mostly CJK characters without Japanese-specific ones, it's Chinese
        if chinese_count as f32 / total_chars as f32 > 0.3 {
            return Language::Chinese;
        }

        // If mostly Latin characters, it's English
        if latin_count as f32 / total_chars as f32 > 0.5 {
            return Language::English;
        }

        Language::Unknown
    }
}


// ============================================================================
// Jieba Chinese Tokenizer
// ============================================================================

/// Chinese tokenizer using jieba-rs
#[derive(Clone)]
pub struct JiebaTokenizer {
    jieba: Arc<jieba_rs::Jieba>,
}

impl JiebaTokenizer {
    /// Create a new JiebaTokenizer with default dictionary
    pub fn new() -> Self {
        Self {
            jieba: Arc::new(jieba_rs::Jieba::new()),
        }
    }

    /// Tokenize Chinese text into words
    pub fn tokenize(&self, text: &str) -> Vec<String> {
        self.jieba
            .cut(text, true) // Use HMM mode for better accuracy
            .into_iter()
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    /// Add a custom word to the dictionary
    pub fn add_word(&self, word: &str, freq: Option<usize>, tag: Option<&str>) {
        // Note: jieba_rs::Jieba doesn't support add_word on shared reference
        // This would require interior mutability or a different approach
        let _ = (word, freq, tag);
    }
}

impl Default for JiebaTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for JiebaTokenizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JiebaTokenizer").finish()
    }
}

/// Token stream for Jieba tokenizer (Tantivy integration)
pub struct JiebaTokenStream {
    tokens: Vec<String>,
    index: usize,
    offset: usize,
    token: Token,
}

impl TokenStream for JiebaTokenStream {
    fn advance(&mut self) -> bool {
        if self.index >= self.tokens.len() {
            return false;
        }

        let text = &self.tokens[self.index];
        self.token = Token {
            offset_from: self.offset,
            offset_to: self.offset + text.len(),
            position: self.index,
            text: text.clone(),
            position_length: 1,
        };
        self.offset += text.len();
        self.index += 1;
        true
    }

    fn token(&self) -> &Token {
        &self.token
    }

    fn token_mut(&mut self) -> &mut Token {
        &mut self.token
    }
}

impl TantivyTokenizer for JiebaTokenizer {
    type TokenStream<'a> = JiebaTokenStream;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        let tokens = self.tokenize(text);
        JiebaTokenStream {
            tokens,
            index: 0,
            offset: 0,
            token: Token::default(),
        }
    }
}


// ============================================================================
// Lindera Japanese Tokenizer (Optional)
// ============================================================================

#[cfg(feature = "japanese")]
pub mod japanese {
    use super::*;
    use lindera::tokenizer::{Tokenizer as LinderaTokenizerInner, TokenizerConfig};
    use lindera::mode::Mode;

    /// Japanese tokenizer using lindera
    pub struct LinderaTokenizer {
        tokenizer: LinderaTokenizerInner,
    }

    impl LinderaTokenizer {
        /// Create a new LinderaTokenizer with default configuration
        pub fn new() -> Result<Self, lindera::LinderaError> {
            let config = TokenizerConfig {
                mode: Mode::Normal,
                ..Default::default()
            };
            let tokenizer = LinderaTokenizerInner::with_config(config)?;
            Ok(Self { tokenizer })
        }

        /// Tokenize Japanese text into words
        pub fn tokenize(&self, text: &str) -> Vec<String> {
            self.tokenizer
                .tokenize(text)
                .unwrap_or_default()
                .into_iter()
                .map(|t| t.text.to_string())
                .collect()
        }
    }

    impl Default for LinderaTokenizer {
        fn default() -> Self {
            Self::new().expect("Failed to create LinderaTokenizer")
        }
    }

    impl std::fmt::Debug for LinderaTokenizer {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("LinderaTokenizer").finish()
        }
    }

    /// Token stream for Lindera tokenizer (Tantivy integration)
    pub struct LinderaTokenStream {
        tokens: Vec<String>,
        index: usize,
        offset: usize,
        token: Token,
    }

    impl TokenStream for LinderaTokenStream {
        fn advance(&mut self) -> bool {
            if self.index >= self.tokens.len() {
                return false;
            }

            let text = &self.tokens[self.index];
            self.token = Token {
                offset_from: self.offset,
                offset_to: self.offset + text.len(),
                position: self.index,
                text: text.clone(),
                position_length: 1,
            };
            self.offset += text.len();
            self.index += 1;
            true
        }

        fn token(&self) -> &Token {
            &self.token
        }

        fn token_mut(&mut self) -> &mut Token {
            &mut self.token
        }
    }

    impl TantivyTokenizer for LinderaTokenizer {
        type TokenStream<'a> = LinderaTokenStream;

        fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
            let tokens = self.tokenize(text);
            LinderaTokenStream {
                tokens,
                index: 0,
                offset: 0,
                token: Token::default(),
            }
        }
    }
}

#[cfg(feature = "japanese")]
pub use japanese::LinderaTokenizer;


// ============================================================================
// Simple Tokenizer (English and other languages)
// ============================================================================

/// Simple tokenizer for English and other Latin-based languages
/// Splits on whitespace and punctuation, converts to lowercase
#[derive(Debug, Clone, Default)]
pub struct SimpleTokenizer;

impl SimpleTokenizer {
    pub fn new() -> Self {
        Self
    }

    /// Tokenize text by splitting on whitespace and punctuation
    pub fn tokenize(&self, text: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut current_token = String::new();

        for ch in text.chars() {
            if ch.is_alphanumeric() || ch == '_' {
                current_token.push(ch.to_ascii_lowercase());
            } else if !current_token.is_empty() {
                tokens.push(std::mem::take(&mut current_token));
            }
        }

        if !current_token.is_empty() {
            tokens.push(current_token);
        }

        tokens
    }
}

/// Token stream for Simple tokenizer (Tantivy integration)
pub struct SimpleTokenStream {
    tokens: Vec<String>,
    index: usize,
    offset: usize,
    token: Token,
}

impl TokenStream for SimpleTokenStream {
    fn advance(&mut self) -> bool {
        if self.index >= self.tokens.len() {
            return false;
        }

        let text = &self.tokens[self.index];
        self.token = Token {
            offset_from: self.offset,
            offset_to: self.offset + text.len(),
            position: self.index,
            text: text.clone(),
            position_length: 1,
        };
        self.offset += text.len();
        self.index += 1;
        true
    }

    fn token(&self) -> &Token {
        &self.token
    }

    fn token_mut(&mut self) -> &mut Token {
        &mut self.token
    }
}

impl TantivyTokenizer for SimpleTokenizer {
    type TokenStream<'a> = SimpleTokenStream;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        let tokens = self.tokenize(text);
        SimpleTokenStream {
            tokens,
            index: 0,
            offset: 0,
            token: Token::default(),
        }
    }
}


// ============================================================================
// Multilingual Tokenizer
// ============================================================================

/// Multilingual tokenizer that automatically detects language and uses
/// the appropriate tokenizer
#[derive(Clone)]
pub struct MultilingualTokenizer {
    chinese_tokenizer: JiebaTokenizer,
    english_tokenizer: SimpleTokenizer,
    language_detector: LanguageDetector,
    #[cfg(feature = "japanese")]
    japanese_tokenizer: Option<japanese::LinderaTokenizer>,
}

impl MultilingualTokenizer {
    /// Create a new MultilingualTokenizer
    pub fn new() -> Self {
        Self {
            chinese_tokenizer: JiebaTokenizer::new(),
            english_tokenizer: SimpleTokenizer::new(),
            language_detector: LanguageDetector::new(),
            #[cfg(feature = "japanese")]
            japanese_tokenizer: japanese::LinderaTokenizer::new().ok(),
        }
    }

    /// Tokenize text using the appropriate tokenizer based on detected language
    pub fn tokenize(&self, text: &str) -> Vec<String> {
        let lang = self.language_detector.detect(text);
        self.tokenize_with_language(text, lang)
    }

    /// Tokenize text with a specific language
    pub fn tokenize_with_language(&self, text: &str, lang: Language) -> Vec<String> {
        match lang {
            Language::Chinese => self.chinese_tokenizer.tokenize(text),
            #[cfg(feature = "japanese")]
            Language::Japanese => {
                if let Some(ref tokenizer) = self.japanese_tokenizer {
                    tokenizer.tokenize(text)
                } else {
                    // Fallback to simple tokenizer if Japanese tokenizer not available
                    self.english_tokenizer.tokenize(text)
                }
            }
            #[cfg(not(feature = "japanese"))]
            Language::Japanese => {
                // Fallback to Chinese tokenizer for Japanese (CJK characters)
                self.chinese_tokenizer.tokenize(text)
            }
            Language::English | Language::Unknown => self.english_tokenizer.tokenize(text),
        }
    }

    /// Get the detected language for text
    pub fn detect_language(&self, text: &str) -> Language {
        self.language_detector.detect(text)
    }
}

impl Default for MultilingualTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for MultilingualTokenizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MultilingualTokenizer").finish()
    }
}

/// Token stream for Multilingual tokenizer (Tantivy integration)
pub struct MultilingualTokenStream {
    tokens: Vec<String>,
    index: usize,
    offset: usize,
    token: Token,
}

impl TokenStream for MultilingualTokenStream {
    fn advance(&mut self) -> bool {
        if self.index >= self.tokens.len() {
            return false;
        }

        let text = &self.tokens[self.index];
        self.token = Token {
            offset_from: self.offset,
            offset_to: self.offset + text.len(),
            position: self.index,
            text: text.clone(),
            position_length: 1,
        };
        self.offset += text.len();
        self.index += 1;
        true
    }

    fn token(&self) -> &Token {
        &self.token
    }

    fn token_mut(&mut self) -> &mut Token {
        &mut self.token
    }
}

impl TantivyTokenizer for MultilingualTokenizer {
    type TokenStream<'a> = MultilingualTokenStream;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        let tokens = self.tokenize(text);
        MultilingualTokenStream {
            tokens,
            index: 0,
            offset: 0,
            token: Token::default(),
        }
    }
}


// ============================================================================
// Tantivy Tokenizer Registration
// ============================================================================

use tantivy::tokenizer::{LowerCaser, RemoveLongFilter, TextAnalyzer, TokenizerManager};

/// Register all multilingual tokenizers with a Tantivy index
pub fn register_tokenizers(tokenizer_manager: &TokenizerManager) {
    // Register Chinese tokenizer
    tokenizer_manager.register(
        "chinese",
        TextAnalyzer::builder(JiebaTokenizer::new())
            .filter(LowerCaser)
            .filter(RemoveLongFilter::limit(40))
            .build(),
    );

    // Register simple tokenizer for English
    tokenizer_manager.register(
        "simple",
        TextAnalyzer::builder(SimpleTokenizer::new())
            .filter(LowerCaser)
            .filter(RemoveLongFilter::limit(40))
            .build(),
    );

    // Register multilingual tokenizer (auto-detect)
    tokenizer_manager.register(
        "multilingual",
        TextAnalyzer::builder(MultilingualTokenizer::new())
            .filter(LowerCaser)
            .filter(RemoveLongFilter::limit(40))
            .build(),
    );

    #[cfg(feature = "japanese")]
    {
        if let Ok(japanese_tokenizer) = japanese::LinderaTokenizer::new() {
            tokenizer_manager.register(
                "japanese",
                TextAnalyzer::builder(japanese_tokenizer)
                    .filter(LowerCaser)
                    .filter(RemoveLongFilter::limit(40))
                    .build(),
            );
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_language_detection_chinese() {
        let detector = LanguageDetector::new();
        assert_eq!(detector.detect("这是一个测试"), Language::Chinese);
        assert_eq!(detector.detect("人工智能文件系统"), Language::Chinese);
    }

    #[test]
    fn test_language_detection_english() {
        let detector = LanguageDetector::new();
        assert_eq!(detector.detect("This is a test"), Language::English);
        assert_eq!(detector.detect("Hello World"), Language::English);
    }

    #[test]
    fn test_language_detection_mixed() {
        let detector = LanguageDetector::new();
        // Mixed text with more Chinese characters
        assert_eq!(detector.detect("这是test测试"), Language::Chinese);
    }

    #[test]
    fn test_jieba_tokenizer() {
        let tokenizer = JiebaTokenizer::new();
        let tokens = tokenizer.tokenize("我爱北京天安门");
        assert!(!tokens.is_empty());
        // Jieba should segment this into meaningful words, not single characters
        assert!(tokens.len() < "我爱北京天安门".chars().count());
    }

    #[test]
    fn test_simple_tokenizer() {
        let tokenizer = SimpleTokenizer::new();
        let tokens = tokenizer.tokenize("Hello, World! This is a test.");
        assert_eq!(tokens, vec!["hello", "world", "this", "is", "a", "test"]);
    }

    #[test]
    fn test_multilingual_tokenizer_chinese() {
        let tokenizer = MultilingualTokenizer::new();
        let tokens = tokenizer.tokenize("人工智能");
        assert!(!tokens.is_empty());
        // Should use Chinese tokenizer and produce meaningful segments
        assert!(tokens.len() <= 2); // "人工智能" should be 1-2 tokens, not 4
    }

    #[test]
    fn test_multilingual_tokenizer_english() {
        let tokenizer = MultilingualTokenizer::new();
        let tokens = tokenizer.tokenize("artificial intelligence");
        assert_eq!(tokens, vec!["artificial", "intelligence"]);
    }
}
