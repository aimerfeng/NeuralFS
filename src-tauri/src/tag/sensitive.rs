//! Sensitive Tag Detection
//!
//! Detects potentially sensitive tags that require user confirmation
//! before being used in search ranking or displayed prominently.
//!
//! # Requirements
//! - 5.5: Sensitive tag detection
//! - 13.4: Privacy protection for sensitive content
//! - UI/UX Design: Tags requiring confirmation

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Sensitivity level for tags
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SensitivityLevel {
    /// Not sensitive - can be used freely
    None,
    /// Low sensitivity - may want user awareness
    Low,
    /// Medium sensitivity - should prompt for confirmation
    Medium,
    /// High sensitivity - requires explicit confirmation
    High,
}

/// A pattern for detecting sensitive tags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitivePattern {
    /// Pattern name for identification
    pub name: String,
    /// Keywords that trigger this pattern
    pub keywords: Vec<String>,
    /// Sensitivity level when matched
    pub level: SensitivityLevel,
    /// Description of why this is sensitive
    pub reason: String,
}

/// Sensitive tag detector
///
/// Analyzes tag names and content to determine if they might be sensitive
/// and require user confirmation before being applied or displayed.
pub struct SensitiveTagDetector {
    /// Patterns for detecting sensitive content
    patterns: Vec<SensitivePattern>,
    /// Cached keyword set for fast lookup
    keyword_cache: HashSet<String>,
}

impl SensitiveTagDetector {
    /// Create a new SensitiveTagDetector with default patterns
    pub fn new() -> Self {
        let patterns = Self::default_patterns();
        let keyword_cache = patterns
            .iter()
            .flat_map(|p| p.keywords.iter().cloned())
            .collect();

        Self {
            patterns,
            keyword_cache,
        }
    }

    /// Create with custom patterns
    pub fn with_patterns(patterns: Vec<SensitivePattern>) -> Self {
        let keyword_cache = patterns
            .iter()
            .flat_map(|p| p.keywords.iter().cloned())
            .collect();

        Self {
            patterns,
            keyword_cache,
        }
    }

    /// Get default sensitive patterns
    fn default_patterns() -> Vec<SensitivePattern> {
        vec![
            // Personal/Private information
            SensitivePattern {
                name: "personal_info".to_string(),
                keywords: vec![
                    "personal".to_string(),
                    "private".to_string(),
                    "confidential".to_string(),
                    "secret".to_string(),
                    "password".to_string(),
                    "credential".to_string(),
                ],
                level: SensitivityLevel::High,
                reason: "May contain personal or confidential information".to_string(),
            },
            // Financial information
            SensitivePattern {
                name: "financial".to_string(),
                keywords: vec![
                    "bank".to_string(),
                    "account".to_string(),
                    "tax".to_string(),
                    "salary".to_string(),
                    "income".to_string(),
                    "investment".to_string(),
                    "credit".to_string(),
                ],
                level: SensitivityLevel::High,
                reason: "May contain financial information".to_string(),
            },
            // Health information
            SensitivePattern {
                name: "health".to_string(),
                keywords: vec![
                    "medical".to_string(),
                    "health".to_string(),
                    "diagnosis".to_string(),
                    "prescription".to_string(),
                    "treatment".to_string(),
                    "hospital".to_string(),
                    "doctor".to_string(),
                ],
                level: SensitivityLevel::High,
                reason: "May contain health-related information".to_string(),
            },
            // Legal documents
            SensitivePattern {
                name: "legal".to_string(),
                keywords: vec![
                    "legal".to_string(),
                    "contract".to_string(),
                    "agreement".to_string(),
                    "lawsuit".to_string(),
                    "attorney".to_string(),
                    "court".to_string(),
                ],
                level: SensitivityLevel::Medium,
                reason: "May contain legal documents".to_string(),
            },
            // Work-related sensitive
            SensitivePattern {
                name: "work_sensitive".to_string(),
                keywords: vec![
                    "nda".to_string(),
                    "proprietary".to_string(),
                    "internal".to_string(),
                    "restricted".to_string(),
                    "classified".to_string(),
                ],
                level: SensitivityLevel::High,
                reason: "May contain work-related sensitive information".to_string(),
            },
            // Identity documents
            SensitivePattern {
                name: "identity".to_string(),
                keywords: vec![
                    "passport".to_string(),
                    "license".to_string(),
                    "ssn".to_string(),
                    "social security".to_string(),
                    "id card".to_string(),
                    "birth certificate".to_string(),
                ],
                level: SensitivityLevel::High,
                reason: "May contain identity documents".to_string(),
            },
            // Relationship/Family
            SensitivePattern {
                name: "relationship".to_string(),
                keywords: vec![
                    "divorce".to_string(),
                    "custody".to_string(),
                    "will".to_string(),
                    "testament".to_string(),
                    "inheritance".to_string(),
                ],
                level: SensitivityLevel::Medium,
                reason: "May contain sensitive family/relationship information".to_string(),
            },
        ]
    }

    /// Check if a tag name is potentially sensitive
    pub fn check_sensitivity(&self, tag_name: &str) -> SensitivityLevel {
        let tag_lower = tag_name.to_lowercase();

        // Quick check against keyword cache
        let words: Vec<&str> = tag_lower.split_whitespace().collect();
        let has_keyword = words.iter().any(|w| self.keyword_cache.contains(*w))
            || self.keyword_cache.iter().any(|kw| tag_lower.contains(kw));

        if !has_keyword {
            return SensitivityLevel::None;
        }

        // Find the highest sensitivity level among matching patterns
        let mut max_level = SensitivityLevel::None;

        for pattern in &self.patterns {
            for keyword in &pattern.keywords {
                if tag_lower.contains(keyword) {
                    if pattern.level as u8 > max_level as u8 {
                        max_level = pattern.level;
                    }
                }
            }
        }

        max_level
    }

    /// Get detailed sensitivity analysis for a tag
    pub fn analyze(&self, tag_name: &str) -> SensitivityAnalysis {
        let tag_lower = tag_name.to_lowercase();
        let mut matched_patterns = Vec::new();
        let mut max_level = SensitivityLevel::None;

        for pattern in &self.patterns {
            let matched_keywords: Vec<String> = pattern
                .keywords
                .iter()
                .filter(|kw| tag_lower.contains(kw.as_str()))
                .cloned()
                .collect();

            if !matched_keywords.is_empty() {
                if pattern.level as u8 > max_level as u8 {
                    max_level = pattern.level;
                }
                matched_patterns.push(MatchedPattern {
                    pattern_name: pattern.name.clone(),
                    matched_keywords,
                    level: pattern.level,
                    reason: pattern.reason.clone(),
                });
            }
        }

        SensitivityAnalysis {
            tag_name: tag_name.to_string(),
            overall_level: max_level,
            matched_patterns,
            requires_confirmation: max_level != SensitivityLevel::None,
        }
    }

    /// Check multiple tags at once
    pub fn check_batch(&self, tag_names: &[&str]) -> Vec<(String, SensitivityLevel)> {
        tag_names
            .iter()
            .map(|name| (name.to_string(), self.check_sensitivity(name)))
            .collect()
    }

    /// Add a custom pattern
    pub fn add_pattern(&mut self, pattern: SensitivePattern) {
        for keyword in &pattern.keywords {
            self.keyword_cache.insert(keyword.clone());
        }
        self.patterns.push(pattern);
    }

    /// Remove a pattern by name
    pub fn remove_pattern(&mut self, pattern_name: &str) {
        self.patterns.retain(|p| p.name != pattern_name);
        // Rebuild keyword cache
        self.keyword_cache = self
            .patterns
            .iter()
            .flat_map(|p| p.keywords.iter().cloned())
            .collect();
    }

    /// Get all patterns
    pub fn patterns(&self) -> &[SensitivePattern] {
        &self.patterns
    }
}

impl Default for SensitiveTagDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Detailed sensitivity analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitivityAnalysis {
    /// The analyzed tag name
    pub tag_name: String,
    /// Overall sensitivity level
    pub overall_level: SensitivityLevel,
    /// Patterns that matched
    pub matched_patterns: Vec<MatchedPattern>,
    /// Whether this tag requires user confirmation
    pub requires_confirmation: bool,
}

/// A pattern that matched during analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchedPattern {
    /// Name of the pattern
    pub pattern_name: String,
    /// Keywords that matched
    pub matched_keywords: Vec<String>,
    /// Sensitivity level of this pattern
    pub level: SensitivityLevel,
    /// Reason for sensitivity
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_sensitivity() {
        let detector = SensitiveTagDetector::new();
        
        assert_eq!(detector.check_sensitivity("Documents"), SensitivityLevel::None);
        assert_eq!(detector.check_sensitivity("Work"), SensitivityLevel::None);
        assert_eq!(detector.check_sensitivity("Photos"), SensitivityLevel::None);
    }

    #[test]
    fn test_high_sensitivity() {
        let detector = SensitiveTagDetector::new();
        
        assert_eq!(detector.check_sensitivity("Personal Documents"), SensitivityLevel::High);
        assert_eq!(detector.check_sensitivity("Bank Statements"), SensitivityLevel::High);
        assert_eq!(detector.check_sensitivity("Medical Records"), SensitivityLevel::High);
        assert_eq!(detector.check_sensitivity("Passwords"), SensitivityLevel::High);
    }

    #[test]
    fn test_medium_sensitivity() {
        let detector = SensitiveTagDetector::new();
        
        assert_eq!(detector.check_sensitivity("Legal Contracts"), SensitivityLevel::Medium);
        assert_eq!(detector.check_sensitivity("Divorce Papers"), SensitivityLevel::Medium);
    }

    #[test]
    fn test_case_insensitive() {
        let detector = SensitiveTagDetector::new();
        
        assert_eq!(detector.check_sensitivity("PERSONAL"), SensitivityLevel::High);
        assert_eq!(detector.check_sensitivity("Personal"), SensitivityLevel::High);
        assert_eq!(detector.check_sensitivity("personal"), SensitivityLevel::High);
    }

    #[test]
    fn test_analysis() {
        let detector = SensitiveTagDetector::new();
        
        let analysis = detector.analyze("Personal Bank Statements");
        
        assert_eq!(analysis.overall_level, SensitivityLevel::High);
        assert!(analysis.requires_confirmation);
        assert!(!analysis.matched_patterns.is_empty());
    }

    #[test]
    fn test_batch_check() {
        let detector = SensitiveTagDetector::new();
        
        let results = detector.check_batch(&["Documents", "Personal", "Photos"]);
        
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].1, SensitivityLevel::None);
        assert_eq!(results[1].1, SensitivityLevel::High);
        assert_eq!(results[2].1, SensitivityLevel::None);
    }

    #[test]
    fn test_custom_pattern() {
        let mut detector = SensitiveTagDetector::new();
        
        // Add custom pattern
        detector.add_pattern(SensitivePattern {
            name: "custom".to_string(),
            keywords: vec!["custom_sensitive".to_string()],
            level: SensitivityLevel::Medium,
            reason: "Custom sensitive pattern".to_string(),
        });
        
        assert_eq!(detector.check_sensitivity("custom_sensitive"), SensitivityLevel::Medium);
    }
}
