//! Local Inference Engine for NeuralFS
//!
//! This module provides local inference capabilities:
//! - Query embedding generation
//! - Tag matching
//! - Intent parsing
//! - Context-enhanced prompt generation for cloud
//!
//! **Validates: Requirements 11.2, 11.4**

use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::core::types::search::SearchIntent;
use crate::core::types::tag::Tag;
use crate::embeddings::EmbeddingEngine;
use crate::search::IntentParser;

use super::error::{InferenceError, InferenceResult};
use super::types::{InferenceContext, InferenceRequest};

/// Local inference engine for embedding generation and intent parsing
pub struct LocalInferenceEngine {
    /// Embedding engine for vector generation
    embedding_engine: Arc<EmbeddingEngine>,
    
    /// Intent parser for query classification
    intent_parser: IntentParser,
    
    /// Tag matcher for local tag matching
    tag_matcher: TagMatcher,
}

/// Result of local inference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalInferenceResult {
    /// Query embedding vector
    pub query_embedding: Vec<f32>,
    
    /// Matched tags with scores
    pub tag_matches: Vec<TagMatch>,
    
    /// Parsed search intent
    pub intent: SearchIntent,
    
    /// Generated cloud prompt (context-enhanced)
    pub cloud_prompt: String,
    
    /// Inference duration in milliseconds
    pub duration_ms: u64,
}

/// Tag match result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagMatch {
    /// Matched tag
    pub tag: Tag,
    
    /// Match score (0.0 - 1.0)
    pub score: f32,
    
    /// Match type
    pub match_type: TagMatchType,
}

/// Type of tag match
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TagMatchType {
    /// Exact name match
    ExactName,
    /// Partial name match
    PartialName,
    /// Semantic similarity match
    Semantic,
    /// Keyword match
    Keyword,
}

impl LocalInferenceEngine {
    /// Create a new local inference engine
    pub fn new(embedding_engine: Arc<EmbeddingEngine>) -> Self {
        Self {
            embedding_engine,
            intent_parser: IntentParser::new(),
            tag_matcher: TagMatcher::new(),
        }
    }
    
    /// Perform local inference on a request
    ///
    /// This generates embeddings, matches tags, parses intent, and creates
    /// a context-enhanced prompt for cloud inference.
    pub async fn infer(&self, request: &InferenceRequest) -> InferenceResult<LocalInferenceResult> {
        let start = Instant::now();
        
        // 1. Generate query embedding
        let query_embedding = self.embedding_engine
            .embed_text_content(&request.query)
            .await
            .map_err(|e| InferenceError::EmbeddingFailed {
                reason: e.to_string(),
            })?;
        
        // 2. Match tags from context
        let tag_matches = self.tag_matcher
            .match_tags(&request.query, &request.context.relevant_tags);
        
        // 3. Parse intent
        let intent_result = self.intent_parser.parse(&request.query);
        let intent = intent_result.intent;
        
        // 4. Generate context-enhanced cloud prompt
        let cloud_prompt = self.generate_cloud_prompt(
            &request.query,
            &tag_matches,
            &intent,
            &request.context,
        );
        
        let duration_ms = start.elapsed().as_millis() as u64;
        
        Ok(LocalInferenceResult {
            query_embedding,
            tag_matches,
            intent,
            cloud_prompt,
            duration_ms,
        })
    }
    
    /// Generate a context-enhanced prompt for cloud inference
    ///
    /// This creates a structured prompt that includes:
    /// - The original query
    /// - Relevant tags
    /// - Parsed intent
    /// - User history context
    /// - File structure context
    fn generate_cloud_prompt(
        &self,
        query: &str,
        tag_matches: &[TagMatch],
        intent: &SearchIntent,
        context: &InferenceContext,
    ) -> String {
        let mut prompt = String::new();
        
        // Add query
        prompt.push_str(&format!("用户查询: \"{}\"\n\n", query));
        
        // Add context section
        prompt.push_str("上下文信息:\n");
        
        // Add matched tags
        if !tag_matches.is_empty() {
            let tag_names: Vec<&str> = tag_matches
                .iter()
                .map(|tm| tm.tag.name.as_str())
                .collect();
            prompt.push_str(&format!("- 相关标签: {}\n", tag_names.join(", ")));
        } else {
            prompt.push_str("- 相关标签: 无\n");
        }
        
        // Add parsed intent
        let intent_desc = match intent {
            SearchIntent::FindFile { file_type_hint, time_hint } => {
                let mut desc = "查找文件".to_string();
                if let Some(ft) = file_type_hint {
                    desc.push_str(&format!(" (类型: {:?})", ft));
                }
                if time_hint.is_some() {
                    desc.push_str(" (有时间限制)");
                }
                desc
            }
            SearchIntent::FindContent { content_type, need_location } => {
                let mut desc = "查找内容".to_string();
                if let Some(ct) = content_type {
                    desc.push_str(&format!(" (类型: {:?})", ct));
                }
                if *need_location {
                    desc.push_str(" (需要定位)");
                }
                desc
            }
            SearchIntent::Ambiguous { .. } => "意图不明确".to_string(),
        };
        prompt.push_str(&format!("- 解析意图: {}\n", intent_desc));
        
        // Add recent files count
        let recent_count = context.user_history.recent_files.len();
        prompt.push_str(&format!("- 最近访问文件数: {}\n", recent_count));
        
        // Add file structure summary if available
        if let Some(ref fs) = context.file_structure {
            prompt.push_str(&format!("- 文件结构: {}\n", fs.summary));
        } else {
            prompt.push_str("- 文件结构: 无\n");
        }
        
        // Add recent searches if any
        if !context.user_history.recent_searches.is_empty() {
            let recent: Vec<&str> = context.user_history.recent_searches
                .iter()
                .take(3)
                .map(|s| s.as_str())
                .collect();
            prompt.push_str(&format!("- 最近搜索: {}\n", recent.join(", ")));
        }
        
        // Add instruction
        prompt.push_str("\n请分析用户意图并提供搜索建议。");
        
        prompt
    }
    
    /// Get the intent parser for direct access
    pub fn intent_parser(&self) -> &IntentParser {
        &self.intent_parser
    }
    
    /// Get the tag matcher for direct access
    pub fn tag_matcher(&self) -> &TagMatcher {
        &self.tag_matcher
    }
}

/// Tag matcher for matching query terms to tags
#[derive(Debug, Clone)]
pub struct TagMatcher {
    /// Minimum score threshold for matches
    min_score_threshold: f32,
}

impl Default for TagMatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl TagMatcher {
    /// Create a new tag matcher
    pub fn new() -> Self {
        Self {
            min_score_threshold: 0.3,
        }
    }
    
    /// Create a tag matcher with custom threshold
    pub fn with_threshold(threshold: f32) -> Self {
        Self {
            min_score_threshold: threshold,
        }
    }
    
    /// Match query against available tags
    pub fn match_tags(&self, query: &str, available_tags: &[Tag]) -> Vec<TagMatch> {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();
        
        let mut matches: Vec<TagMatch> = available_tags
            .iter()
            .filter_map(|tag| {
                let (score, match_type) = self.calculate_match_score(&query_lower, &query_words, tag);
                if score >= self.min_score_threshold {
                    Some(TagMatch {
                        tag: tag.clone(),
                        score,
                        match_type,
                    })
                } else {
                    None
                }
            })
            .collect();
        
        // Sort by score descending
        matches.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        
        matches
    }
    
    /// Calculate match score between query and tag
    fn calculate_match_score(&self, query: &str, query_words: &[&str], tag: &Tag) -> (f32, TagMatchType) {
        let tag_name_lower = tag.name.to_lowercase();
        
        // Check for exact match
        if query.contains(&tag_name_lower) || tag_name_lower.contains(query) {
            return (1.0, TagMatchType::ExactName);
        }
        
        // Check for partial match
        for word in query_words {
            if tag_name_lower.contains(word) || word.contains(&tag_name_lower) {
                return (0.8, TagMatchType::PartialName);
            }
        }
        
        // Check display names
        for (_, display_name) in &tag.display_name {
            let display_lower = display_name.to_lowercase();
            if query.contains(&display_lower) || display_lower.contains(query) {
                return (0.9, TagMatchType::ExactName);
            }
            for word in query_words {
                if display_lower.contains(word) {
                    return (0.7, TagMatchType::PartialName);
                }
            }
        }
        
        // Check for keyword overlap (simple word matching)
        let tag_words: Vec<&str> = tag_name_lower.split(|c: char| !c.is_alphanumeric()).collect();
        let overlap_count = query_words
            .iter()
            .filter(|qw| tag_words.iter().any(|tw| tw.contains(*qw) || qw.contains(tw)))
            .count();
        
        if overlap_count > 0 {
            let score = (overlap_count as f32 / query_words.len().max(1) as f32) * 0.6;
            return (score, TagMatchType::Keyword);
        }
        
        (0.0, TagMatchType::Keyword)
    }
    
    /// Set the minimum score threshold
    pub fn set_threshold(&mut self, threshold: f32) {
        self.min_score_threshold = threshold.clamp(0.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use uuid::Uuid;
    use chrono::Utc;
    use crate::core::types::tag::TagType;

    fn create_test_tag(name: &str) -> Tag {
        Tag {
            id: Uuid::now_v7(),
            name: name.to_string(),
            display_name: HashMap::new(),
            parent_id: None,
            tag_type: TagType::Custom,
            color: "#000000".to_string(),
            icon: None,
            is_system: false,
            created_at: Utc::now(),
            usage_count: 0,
        }
    }

    #[test]
    fn test_tag_matcher_exact_match() {
        let matcher = TagMatcher::new();
        let tags = vec![
            create_test_tag("work"),
            create_test_tag("personal"),
            create_test_tag("project"),
        ];
        
        let matches = matcher.match_tags("work documents", &tags);
        assert!(!matches.is_empty());
        assert_eq!(matches[0].tag.name, "work");
        assert_eq!(matches[0].match_type, TagMatchType::ExactName);
    }

    #[test]
    fn test_tag_matcher_partial_match() {
        let matcher = TagMatcher::new();
        let tags = vec![
            create_test_tag("machine_learning"),
            create_test_tag("deep_learning"),
        ];
        
        let matches = matcher.match_tags("learning resources", &tags);
        assert!(!matches.is_empty());
        // Both should match due to "learning"
        assert!(matches.iter().any(|m| m.tag.name == "machine_learning"));
    }

    #[test]
    fn test_tag_matcher_no_match() {
        let matcher = TagMatcher::new();
        let tags = vec![
            create_test_tag("work"),
            create_test_tag("personal"),
        ];
        
        let matches = matcher.match_tags("xyz123", &tags);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_tag_matcher_threshold() {
        let matcher = TagMatcher::with_threshold(0.9);
        let tags = vec![
            create_test_tag("work"),
            create_test_tag("working"),
        ];
        
        // With high threshold, only exact matches should pass
        let matches = matcher.match_tags("work", &tags);
        assert!(!matches.is_empty());
        assert!(matches[0].score >= 0.9);
    }
}
