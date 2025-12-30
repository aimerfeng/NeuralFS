//! Intent parsing module for NeuralFS
//!
//! This module provides:
//! - Intent classification (file-level vs segment-level)
//! - Query pattern recognition
//! - Clarification question generation for ambiguous queries
//!
//! **Validates: Requirements 2.1**

use serde::{Deserialize, Serialize};

use crate::core::types::chunk::ChunkType;
use crate::core::types::file::FileType;
use crate::core::types::search::{SearchIntent, TimeRange};

/// Intent parser for classifying user search queries
///
/// The parser analyzes query text to determine whether the user is looking for:
/// - A specific file (file-level intent)
/// - A specific content segment within files (segment-level intent)
/// - An ambiguous query that needs clarification
#[derive(Debug, Clone)]
pub struct IntentParser {
    /// Keywords that indicate file-level search intent
    file_keywords: Vec<&'static str>,
    /// Keywords that indicate content/segment-level search intent
    content_keywords: Vec<&'static str>,
    /// File type indicators (extension patterns)
    file_type_patterns: Vec<(&'static str, FileType)>,
    /// Time-related keywords
    time_keywords: Vec<(&'static str, TimeHint)>,
    /// Content type indicators
    content_type_patterns: Vec<(&'static str, ChunkType)>,
}

/// Time hint extracted from query
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeHint {
    Recent,    // "recent", "latest", "new"
    Today,     // "today"
    Yesterday, // "yesterday"
    ThisWeek,  // "this week"
    ThisMonth, // "this month"
    Old,       // "old", "archive"
}

/// Result of intent parsing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentParseResult {
    /// The classified intent
    pub intent: SearchIntent,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Extracted keywords from the query
    pub extracted_keywords: Vec<String>,
    /// Whether the query is considered ambiguous
    pub is_ambiguous: bool,
}

/// Intent classification category
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntentCategory {
    /// User is looking for a file
    File,
    /// User is looking for content within files
    Content,
    /// Intent is unclear
    Ambiguous,
}

impl Default for IntentParser {
    fn default() -> Self {
        Self::new()
    }
}

impl IntentParser {
    /// Create a new IntentParser with default configuration
    pub fn new() -> Self {
        Self {
            file_keywords: vec![
                "file", "document", "find file", "where is", "locate",
                "文件", "文档", "找文件", "在哪", "哪个文件",
                "ファイル", "書類",
            ],
            content_keywords: vec![
                "content", "text", "paragraph", "section", "quote", "mention",
                "says", "contains", "written", "about", "describes",
                "内容", "段落", "提到", "写着", "关于", "描述",
                "内容", "段落", "書いてある",
            ],
            file_type_patterns: vec![
                ("pdf", FileType::Pdf),
                ("document", FileType::TextDocument),
                ("doc", FileType::OfficeDocument),
                ("docx", FileType::OfficeDocument),
                ("image", FileType::Image),
                ("photo", FileType::Image),
                ("picture", FileType::Image),
                ("video", FileType::Video),
                ("movie", FileType::Video),
                ("code", FileType::Code),
                ("source", FileType::Code),
                ("script", FileType::Code),
                ("model", FileType::Model3D),
                ("3d", FileType::Model3D),
                // Chinese
                ("图片", FileType::Image),
                ("照片", FileType::Image),
                ("视频", FileType::Video),
                ("代码", FileType::Code),
                ("文档", FileType::TextDocument),
            ],
            time_keywords: vec![
                ("recent", TimeHint::Recent),
                ("latest", TimeHint::Recent),
                ("new", TimeHint::Recent),
                ("today", TimeHint::Today),
                ("yesterday", TimeHint::Yesterday),
                ("this week", TimeHint::ThisWeek),
                ("this month", TimeHint::ThisMonth),
                ("old", TimeHint::Old),
                ("archive", TimeHint::Old),
                // Chinese
                ("最近", TimeHint::Recent),
                ("最新", TimeHint::Recent),
                ("今天", TimeHint::Today),
                ("昨天", TimeHint::Yesterday),
                ("本周", TimeHint::ThisWeek),
                ("这周", TimeHint::ThisWeek),
                ("本月", TimeHint::ThisMonth),
                ("这个月", TimeHint::ThisMonth),
                ("旧的", TimeHint::Old),
            ],
            content_type_patterns: vec![
                ("paragraph", ChunkType::Paragraph),
                ("heading", ChunkType::Heading),
                ("title", ChunkType::Heading),
                ("code", ChunkType::CodeBlock),
                ("function", ChunkType::CodeBlock),
                ("table", ChunkType::Table),
                ("image", ChunkType::Image),
                ("caption", ChunkType::Caption),
                // Chinese
                ("段落", ChunkType::Paragraph),
                ("标题", ChunkType::Heading),
                ("代码", ChunkType::CodeBlock),
                ("函数", ChunkType::CodeBlock),
                ("表格", ChunkType::Table),
                ("图片", ChunkType::Image),
            ],
        }
    }

    /// Parse a query string and determine the search intent
    ///
    /// # Arguments
    /// * `query` - The user's search query
    ///
    /// # Returns
    /// An `IntentParseResult` containing the classified intent and metadata
    pub fn parse(&self, query: &str) -> IntentParseResult {
        let query_lower = query.to_lowercase();
        
        // Calculate scores for each intent type
        let file_score = self.calculate_file_score(&query_lower);
        let content_score = self.calculate_content_score(&query_lower);
        
        // Extract additional hints
        let file_type_hint = self.extract_file_type(&query_lower);
        let time_hint = self.extract_time_hint(&query_lower);
        let content_type_hint = self.extract_content_type(&query_lower);
        let extracted_keywords = self.extract_keywords(&query_lower);
        
        // Determine intent based on scores
        let (intent, confidence, is_ambiguous) = self.classify_intent(
            file_score,
            content_score,
            file_type_hint,
            time_hint,
            content_type_hint,
            &query_lower,
        );
        
        IntentParseResult {
            intent,
            confidence,
            extracted_keywords,
            is_ambiguous,
        }
    }

    /// Classify the intent category without full parsing
    ///
    /// This is a lightweight method for quick classification
    pub fn classify(&self, query: &str) -> IntentCategory {
        let query_lower = query.to_lowercase();
        let file_score = self.calculate_file_score(&query_lower);
        let content_score = self.calculate_content_score(&query_lower);
        
        // Threshold for ambiguity
        let score_diff = (file_score - content_score).abs();
        
        if score_diff < 0.2 && file_score < 0.5 && content_score < 0.5 {
            IntentCategory::Ambiguous
        } else if file_score > content_score {
            IntentCategory::File
        } else {
            IntentCategory::Content
        }
    }

    /// Check if a query indicates file-level intent
    pub fn is_file_intent(&self, query: &str) -> bool {
        matches!(self.classify(query), IntentCategory::File)
    }

    /// Check if a query indicates content-level intent
    pub fn is_content_intent(&self, query: &str) -> bool {
        matches!(self.classify(query), IntentCategory::Content)
    }

    /// Calculate file-level intent score
    fn calculate_file_score(&self, query: &str) -> f32 {
        let mut score = 0.0f32;
        
        // Check for file keywords
        for keyword in &self.file_keywords {
            if query.contains(keyword) {
                score += 0.3;
            }
        }
        
        // Check for file type patterns
        for (pattern, _) in &self.file_type_patterns {
            if query.contains(pattern) {
                score += 0.2;
            }
        }
        
        // Check for file extension patterns (e.g., ".pdf", ".docx")
        if query.contains('.') && self.has_extension_pattern(query) {
            score += 0.3;
        }
        
        // Check for path-like patterns
        if query.contains('/') || query.contains('\\') {
            score += 0.2;
        }
        
        score.min(1.0)
    }

    /// Calculate content-level intent score
    fn calculate_content_score(&self, query: &str) -> f32 {
        let mut score = 0.0f32;
        
        // Check for content keywords
        for keyword in &self.content_keywords {
            if query.contains(keyword) {
                score += 0.3;
            }
        }
        
        // Check for content type patterns
        for (pattern, _) in &self.content_type_patterns {
            if query.contains(pattern) {
                score += 0.2;
            }
        }
        
        // Check for quotation marks (indicating specific content search)
        if query.contains('"') || query.contains('"') || query.contains('"') {
            score += 0.3;
        }
        
        // Longer queries tend to be content searches
        let word_count = query.split_whitespace().count();
        if word_count > 5 {
            score += 0.1;
        }
        if word_count > 10 {
            score += 0.1;
        }
        
        score.min(1.0)
    }

    /// Check if query contains a file extension pattern
    fn has_extension_pattern(&self, query: &str) -> bool {
        let extensions = [
            ".pdf", ".doc", ".docx", ".txt", ".md",
            ".png", ".jpg", ".jpeg", ".gif",
            ".mp4", ".avi", ".mkv",
            ".rs", ".py", ".js", ".ts",
            ".json", ".yaml", ".xml",
        ];
        
        extensions.iter().any(|ext| query.contains(ext))
    }

    /// Extract file type hint from query
    fn extract_file_type(&self, query: &str) -> Option<FileType> {
        for (pattern, file_type) in &self.file_type_patterns {
            if query.contains(pattern) {
                return Some(*file_type);
            }
        }
        None
    }

    /// Extract time hint from query
    fn extract_time_hint(&self, query: &str) -> Option<TimeHint> {
        for (pattern, hint) in &self.time_keywords {
            if query.contains(pattern) {
                return Some(*hint);
            }
        }
        None
    }

    /// Extract content type hint from query
    fn extract_content_type(&self, query: &str) -> Option<ChunkType> {
        for (pattern, chunk_type) in &self.content_type_patterns {
            if query.contains(pattern) {
                return Some(*chunk_type);
            }
        }
        None
    }

    /// Extract meaningful keywords from query
    fn extract_keywords(&self, query: &str) -> Vec<String> {
        let stop_words = [
            "the", "a", "an", "is", "are", "was", "were", "be", "been",
            "being", "have", "has", "had", "do", "does", "did", "will",
            "would", "could", "should", "may", "might", "must", "shall",
            "can", "need", "dare", "ought", "used", "to", "of", "in",
            "for", "on", "with", "at", "by", "from", "as", "into",
            "through", "during", "before", "after", "above", "below",
            "between", "under", "again", "further", "then", "once",
            "here", "there", "when", "where", "why", "how", "all",
            "each", "few", "more", "most", "other", "some", "such",
            "no", "nor", "not", "only", "own", "same", "so", "than",
            "too", "very", "just", "and", "but", "if", "or", "because",
            "until", "while", "this", "that", "these", "those", "i",
            "me", "my", "myself", "we", "our", "ours", "ourselves",
            "you", "your", "yours", "yourself", "yourselves", "he",
            "him", "his", "himself", "she", "her", "hers", "herself",
            "it", "its", "itself", "they", "them", "their", "theirs",
            "themselves", "what", "which", "who", "whom", "find",
            "search", "look", "get", "show", "give",
        ];
        
        query
            .split(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
            .filter(|word| !word.is_empty())
            .filter(|word| !stop_words.contains(&word.to_lowercase().as_str()))
            .filter(|word| word.len() > 1)
            .map(|s| s.to_string())
            .collect()
    }

    /// Classify intent and generate appropriate SearchIntent
    fn classify_intent(
        &self,
        file_score: f32,
        content_score: f32,
        file_type_hint: Option<FileType>,
        time_hint: Option<TimeHint>,
        content_type_hint: Option<ChunkType>,
        query: &str,
    ) -> (SearchIntent, f32, bool) {
        let score_diff = (file_score - content_score).abs();
        let max_score = file_score.max(content_score);
        
        // Ambiguous case: scores are close and both low
        if score_diff < 0.15 && max_score < 0.4 {
            let clarification_questions = self.generate_clarification_questions(query);
            let possible_intents = vec![
                SearchIntent::FindFile {
                    file_type_hint,
                    time_hint: time_hint.map(|h| self.time_hint_to_range(h)),
                },
                SearchIntent::FindContent {
                    content_type: content_type_hint,
                    need_location: true,
                },
            ];
            
            return (
                SearchIntent::Ambiguous {
                    possible_intents,
                    clarification_questions,
                },
                max_score,
                true,
            );
        }
        
        // File-level intent
        if file_score >= content_score {
            (
                SearchIntent::FindFile {
                    file_type_hint,
                    time_hint: time_hint.map(|h| self.time_hint_to_range(h)),
                },
                file_score,
                false,
            )
        } else {
            // Content-level intent
            (
                SearchIntent::FindContent {
                    content_type: content_type_hint,
                    need_location: true,
                },
                content_score,
                false,
            )
        }
    }

    /// Convert TimeHint to TimeRange
    fn time_hint_to_range(&self, hint: TimeHint) -> TimeRange {
        use chrono::{Duration, Utc};
        
        let now = Utc::now();
        
        match hint {
            TimeHint::Recent => TimeRange {
                start: Some(now - Duration::days(7)),
                end: Some(now),
            },
            TimeHint::Today => TimeRange {
                start: Some(now - Duration::days(1)),
                end: Some(now),
            },
            TimeHint::Yesterday => TimeRange {
                start: Some(now - Duration::days(2)),
                end: Some(now - Duration::days(1)),
            },
            TimeHint::ThisWeek => TimeRange {
                start: Some(now - Duration::days(7)),
                end: Some(now),
            },
            TimeHint::ThisMonth => TimeRange {
                start: Some(now - Duration::days(30)),
                end: Some(now),
            },
            TimeHint::Old => TimeRange {
                start: None,
                end: Some(now - Duration::days(90)),
            },
        }
    }

    /// Generate clarification questions for ambiguous queries
    fn generate_clarification_questions(&self, query: &str) -> Vec<String> {
        let mut questions = Vec::new();
        
        // Basic clarification
        questions.push(format!(
            "Are you looking for a specific file or content within files related to \"{}\"?",
            query
        ));
        
        // File type clarification
        questions.push("What type of file are you looking for? (document, image, code, etc.)".to_string());
        
        // Time-based clarification
        questions.push("Are you looking for recent files or older ones?".to_string());
        
        questions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_intent_detection() {
        let parser = IntentParser::new();
        
        // Clear file-level queries
        assert!(parser.is_file_intent("find the report.pdf file"));
        assert!(parser.is_file_intent("where is my document"));
        assert!(parser.is_file_intent("locate the image file"));
        assert!(parser.is_file_intent("找文件 report"));
    }

    #[test]
    fn test_content_intent_detection() {
        let parser = IntentParser::new();
        
        // Clear content-level queries
        assert!(parser.is_content_intent("find the paragraph that mentions machine learning"));
        assert!(parser.is_content_intent("content about neural networks"));
        assert!(parser.is_content_intent("\"specific quote from document\""));
        assert!(parser.is_content_intent("段落 关于人工智能"));
    }

    #[test]
    fn test_file_type_extraction() {
        let parser = IntentParser::new();
        
        let result = parser.parse("find the pdf document");
        if let SearchIntent::FindFile { file_type_hint, .. } = result.intent {
            assert_eq!(file_type_hint, Some(FileType::Pdf));
        }
        
        let result = parser.parse("show me the image");
        if let SearchIntent::FindFile { file_type_hint, .. } = result.intent {
            assert_eq!(file_type_hint, Some(FileType::Image));
        }
    }

    #[test]
    fn test_time_hint_extraction() {
        let parser = IntentParser::new();
        
        let result = parser.parse("find recent documents");
        if let SearchIntent::FindFile { time_hint, .. } = result.intent {
            assert!(time_hint.is_some());
        }
        
        let result = parser.parse("最近的文件");
        if let SearchIntent::FindFile { time_hint, .. } = result.intent {
            assert!(time_hint.is_some());
        }
    }

    #[test]
    fn test_ambiguous_query_detection() {
        let parser = IntentParser::new();
        
        // Very short/vague queries should be ambiguous
        let result = parser.parse("hello");
        assert!(result.is_ambiguous || result.confidence < 0.5);
    }

    #[test]
    fn test_keyword_extraction() {
        let parser = IntentParser::new();
        
        let result = parser.parse("find the machine learning report from yesterday");
        assert!(result.extracted_keywords.contains(&"machine".to_string()));
        assert!(result.extracted_keywords.contains(&"learning".to_string()));
        assert!(result.extracted_keywords.contains(&"report".to_string()));
        assert!(result.extracted_keywords.contains(&"yesterday".to_string()));
    }

    #[test]
    fn test_classify_method() {
        let parser = IntentParser::new();
        
        assert_eq!(parser.classify("find file report.pdf"), IntentCategory::File);
        assert_eq!(parser.classify("content about AI"), IntentCategory::Content);
    }

    #[test]
    fn test_chinese_queries() {
        let parser = IntentParser::new();
        
        // Chinese file query
        let result = parser.parse("找文件 报告");
        assert!(matches!(result.intent, SearchIntent::FindFile { .. }));
        
        // Chinese content query
        let result = parser.parse("关于机器学习的段落");
        assert!(matches!(result.intent, SearchIntent::FindContent { .. }));
    }
}
