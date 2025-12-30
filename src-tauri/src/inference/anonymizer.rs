//! Data Anonymization for NeuralFS
//!
//! This module provides data anonymization for cloud inference:
//! - Remove sensitive file paths
//! - Replace usernames with placeholders
//! - Strip personal identifiers
//! - Configurable anonymization rules
//!
//! **Validates: Requirements 13.2**

use std::collections::HashSet;
use std::env;
use std::path::Path;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Data anonymizer for removing sensitive information
#[derive(Debug, Clone)]
pub struct DataAnonymizer {
    /// Configuration
    config: AnonymizationConfig,
    
    /// Compiled regex patterns
    patterns: AnonymizationPatterns,
}

/// Anonymization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnonymizationConfig {
    /// Whether to anonymize usernames
    pub anonymize_usernames: bool,
    
    /// Whether to anonymize file paths
    pub anonymize_paths: bool,
    
    /// Whether to anonymize email addresses
    pub anonymize_emails: bool,
    
    /// Whether to anonymize IP addresses
    pub anonymize_ips: bool,
    
    /// Custom patterns to anonymize (regex strings)
    pub custom_patterns: Vec<String>,
    
    /// Words to always preserve (not anonymize)
    pub preserve_words: HashSet<String>,
}

impl Default for AnonymizationConfig {
    fn default() -> Self {
        Self {
            anonymize_usernames: true,
            anonymize_paths: true,
            anonymize_emails: true,
            anonymize_ips: true,
            custom_patterns: Vec::new(),
            preserve_words: HashSet::new(),
        }
    }
}

/// Compiled regex patterns for anonymization
#[derive(Debug, Clone)]
struct AnonymizationPatterns {
    /// Email pattern
    email: Regex,
    
    /// IPv4 pattern
    ipv4: Regex,
    
    /// IPv6 pattern (simplified)
    ipv6: Regex,
    
    /// Windows path pattern
    windows_path: Regex,
    
    /// Unix path pattern
    unix_path: Regex,
    
    /// Custom patterns
    custom: Vec<Regex>,
}

impl Default for AnonymizationPatterns {
    fn default() -> Self {
        Self {
            email: Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}")
                .expect("Invalid email regex"),
            ipv4: Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b")
                .expect("Invalid IPv4 regex"),
            ipv6: Regex::new(r"([0-9a-fA-F]{1,4}:){7}[0-9a-fA-F]{1,4}")
                .expect("Invalid IPv6 regex"),
            windows_path: Regex::new(r"[A-Za-z]:\\[^\s\"\'\n]+")
                .expect("Invalid Windows path regex"),
            unix_path: Regex::new(r"/(?:home|Users|var|tmp|etc)/[^\s\"\'\n]+")
                .expect("Invalid Unix path regex"),
            custom: Vec::new(),
        }
    }
}

impl DataAnonymizer {
    /// Create a new data anonymizer with default configuration
    pub fn new() -> Self {
        Self::with_config(AnonymizationConfig::default())
    }
    
    /// Create a new data anonymizer with custom configuration
    pub fn with_config(config: AnonymizationConfig) -> Self {
        let mut patterns = AnonymizationPatterns::default();
        
        // Compile custom patterns
        for pattern_str in &config.custom_patterns {
            if let Ok(regex) = Regex::new(pattern_str) {
                patterns.custom.push(regex);
            } else {
                tracing::warn!("Invalid custom anonymization pattern: {}", pattern_str);
            }
        }
        
        Self { config, patterns }
    }
    
    /// Anonymize a string by removing sensitive information
    pub fn anonymize(&self, input: &str) -> String {
        let mut result = input.to_string();
        
        // Anonymize usernames
        if self.config.anonymize_usernames {
            result = self.anonymize_usernames(&result);
        }
        
        // Anonymize file paths
        if self.config.anonymize_paths {
            result = self.anonymize_paths(&result);
        }
        
        // Anonymize email addresses
        if self.config.anonymize_emails {
            result = self.anonymize_emails(&result);
        }
        
        // Anonymize IP addresses
        if self.config.anonymize_ips {
            result = self.anonymize_ips(&result);
        }
        
        // Apply custom patterns
        result = self.apply_custom_patterns(&result);
        
        result
    }
    
    /// Anonymize usernames in the input
    fn anonymize_usernames(&self, input: &str) -> String {
        let mut result = input.to_string();
        
        // Get current username from environment
        if let Ok(username) = env::var("USERNAME").or_else(|_| env::var("USER")) {
            if !username.is_empty() && !self.config.preserve_words.contains(&username) {
                result = result.replace(&username, "[USER]");
                // Also replace case-insensitive
                let username_lower = username.to_lowercase();
                result = self.replace_case_insensitive(&result, &username_lower, "[USER]");
            }
        }
        
        // Get home directory and extract username from it
        if let Ok(home) = env::var("HOME").or_else(|_| env::var("USERPROFILE")) {
            if let Some(home_username) = Path::new(&home).file_name() {
                let home_username_str = home_username.to_string_lossy();
                if !home_username_str.is_empty() && !self.config.preserve_words.contains(home_username_str.as_ref()) {
                    result = result.replace(home_username_str.as_ref(), "[USER]");
                }
            }
        }
        
        // Common username patterns in paths
        let username_patterns = [
            (r"\\Users\\([^\\]+)", "[USER]"),
            (r"/home/([^/]+)", "[USER]"),
            (r"/Users/([^/]+)", "[USER]"),
        ];
        
        for (pattern, replacement) in username_patterns {
            if let Ok(regex) = Regex::new(pattern) {
                result = regex.replace_all(&result, |caps: &regex::Captures| {
                    if let Some(username) = caps.get(1) {
                        let username_str = username.as_str();
                        if self.config.preserve_words.contains(username_str) {
                            caps[0].to_string()
                        } else {
                            caps[0].replace(username_str, replacement)
                        }
                    } else {
                        caps[0].to_string()
                    }
                }).to_string();
            }
        }
        
        result
    }
    
    /// Anonymize file paths in the input
    fn anonymize_paths(&self, input: &str) -> String {
        let mut result = input.to_string();
        
        // Replace home directory
        if let Ok(home) = env::var("HOME").or_else(|_| env::var("USERPROFILE")) {
            result = result.replace(&home, "[HOME]");
        }
        
        // Replace Windows paths with sensitive directories
        result = self.patterns.windows_path.replace_all(&result, |caps: &regex::Captures| {
            let path = &caps[0];
            self.anonymize_path_string(path)
        }).to_string();
        
        // Replace Unix paths with sensitive directories
        result = self.patterns.unix_path.replace_all(&result, |caps: &regex::Captures| {
            let path = &caps[0];
            self.anonymize_path_string(path)
        }).to_string();
        
        result
    }
    
    /// Anonymize a single path string
    fn anonymize_path_string(&self, path: &str) -> String {
        let mut result = path.to_string();
        
        // Replace common sensitive directories
        let sensitive_dirs = [
            ("Desktop", "[DESKTOP]"),
            ("Documents", "[DOCUMENTS]"),
            ("Downloads", "[DOWNLOADS]"),
            ("Pictures", "[PICTURES]"),
            ("Videos", "[VIDEOS]"),
            ("Music", "[MUSIC]"),
            ("AppData", "[APPDATA]"),
            (".config", "[CONFIG]"),
            (".local", "[LOCAL]"),
        ];
        
        for (dir, replacement) in sensitive_dirs {
            if result.contains(dir) {
                // Keep the directory structure but anonymize the base
                let parts: Vec<&str> = if result.contains('\\') {
                    result.split('\\').collect()
                } else {
                    result.split('/').collect()
                };
                
                // Find and replace the sensitive directory
                let new_parts: Vec<String> = parts.iter().map(|p| {
                    if *p == dir {
                        replacement.to_string()
                    } else {
                        (*p).to_string()
                    }
                }).collect();
                
                let separator = if result.contains('\\') { "\\" } else { "/" };
                result = new_parts.join(separator);
            }
        }
        
        // Replace user-specific path components
        if let Ok(username) = env::var("USERNAME").or_else(|_| env::var("USER")) {
            result = result.replace(&username, "[USER]");
        }
        
        result
    }
    
    /// Anonymize email addresses in the input
    fn anonymize_emails(&self, input: &str) -> String {
        self.patterns.email.replace_all(input, "[EMAIL]").to_string()
    }
    
    /// Anonymize IP addresses in the input
    fn anonymize_ips(&self, input: &str) -> String {
        let mut result = self.patterns.ipv4.replace_all(input, "[IP]").to_string();
        result = self.patterns.ipv6.replace_all(&result, "[IP]").to_string();
        result
    }
    
    /// Apply custom anonymization patterns
    fn apply_custom_patterns(&self, input: &str) -> String {
        let mut result = input.to_string();
        
        for (i, pattern) in self.patterns.custom.iter().enumerate() {
            result = pattern.replace_all(&result, format!("[CUSTOM_{}]", i)).to_string();
        }
        
        result
    }
    
    /// Case-insensitive replacement
    fn replace_case_insensitive(&self, input: &str, pattern: &str, replacement: &str) -> String {
        if pattern.is_empty() {
            return input.to_string();
        }
        
        let pattern_lower = pattern.to_lowercase();
        let input_lower = input.to_lowercase();
        
        let mut result = String::new();
        let mut last_end = 0;
        
        for (start, _) in input_lower.match_indices(&pattern_lower) {
            result.push_str(&input[last_end..start]);
            result.push_str(replacement);
            last_end = start + pattern.len();
        }
        
        result.push_str(&input[last_end..]);
        result
    }
    
    /// Check if a string contains sensitive information
    pub fn contains_sensitive(&self, input: &str) -> bool {
        // Check for emails
        if self.config.anonymize_emails && self.patterns.email.is_match(input) {
            return true;
        }
        
        // Check for IPs
        if self.config.anonymize_ips && 
           (self.patterns.ipv4.is_match(input) || self.patterns.ipv6.is_match(input)) {
            return true;
        }
        
        // Check for paths
        if self.config.anonymize_paths && 
           (self.patterns.windows_path.is_match(input) || self.patterns.unix_path.is_match(input)) {
            return true;
        }
        
        // Check for username
        if self.config.anonymize_usernames {
            if let Ok(username) = env::var("USERNAME").or_else(|_| env::var("USER")) {
                if input.contains(&username) {
                    return true;
                }
            }
        }
        
        false
    }
    
    /// Get the current configuration
    pub fn config(&self) -> &AnonymizationConfig {
        &self.config
    }
}

impl Default for DataAnonymizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anonymize_email() {
        let anonymizer = DataAnonymizer::new();
        
        let input = "Contact me at user@example.com for more info";
        let result = anonymizer.anonymize(input);
        
        assert!(result.contains("[EMAIL]"));
        assert!(!result.contains("user@example.com"));
    }

    #[test]
    fn test_anonymize_ipv4() {
        let anonymizer = DataAnonymizer::new();
        
        let input = "Server is at 192.168.1.100";
        let result = anonymizer.anonymize(input);
        
        assert!(result.contains("[IP]"));
        assert!(!result.contains("192.168.1.100"));
    }

    #[test]
    fn test_anonymize_windows_path() {
        let anonymizer = DataAnonymizer::new();
        
        let input = r"File is at C:\Users\john\Documents\report.pdf";
        let result = anonymizer.anonymize(input);
        
        // Should anonymize the path
        assert!(result.contains("[USER]") || result.contains("[DOCUMENTS]") || result.contains("[HOME]"));
    }

    #[test]
    fn test_anonymize_unix_path() {
        let anonymizer = DataAnonymizer::new();
        
        let input = "File is at /home/john/documents/report.pdf";
        let result = anonymizer.anonymize(input);
        
        // Should anonymize the path
        assert!(result.contains("[USER]") || result.contains("[HOME]"));
    }

    #[test]
    fn test_contains_sensitive_email() {
        let anonymizer = DataAnonymizer::new();
        
        assert!(anonymizer.contains_sensitive("Contact user@example.com"));
        assert!(!anonymizer.contains_sensitive("No sensitive data here"));
    }

    #[test]
    fn test_contains_sensitive_ip() {
        let anonymizer = DataAnonymizer::new();
        
        assert!(anonymizer.contains_sensitive("Server at 10.0.0.1"));
        assert!(!anonymizer.contains_sensitive("No IP here"));
    }

    #[test]
    fn test_preserve_words() {
        let mut config = AnonymizationConfig::default();
        config.preserve_words.insert("admin".to_string());
        
        let anonymizer = DataAnonymizer::with_config(config);
        
        // "admin" should be preserved even if it looks like a username
        let input = "User admin logged in";
        let result = anonymizer.anonymize(input);
        
        assert!(result.contains("admin"));
    }

    #[test]
    fn test_custom_patterns() {
        let mut config = AnonymizationConfig::default();
        config.custom_patterns.push(r"SECRET_\w+".to_string());
        
        let anonymizer = DataAnonymizer::with_config(config);
        
        let input = "The key is SECRET_ABC123";
        let result = anonymizer.anonymize(input);
        
        assert!(result.contains("[CUSTOM_0]"));
        assert!(!result.contains("SECRET_ABC123"));
    }

    #[test]
    fn test_disabled_anonymization() {
        let config = AnonymizationConfig {
            anonymize_usernames: false,
            anonymize_paths: false,
            anonymize_emails: false,
            anonymize_ips: false,
            custom_patterns: Vec::new(),
            preserve_words: HashSet::new(),
        };
        
        let anonymizer = DataAnonymizer::with_config(config);
        
        let input = "Email: test@example.com, IP: 192.168.1.1";
        let result = anonymizer.anonymize(input);
        
        // Nothing should be anonymized
        assert_eq!(input, result);
    }

    #[test]
    fn test_multiple_sensitive_items() {
        let anonymizer = DataAnonymizer::new();
        
        let input = "Contact user1@example.com or user2@test.org at 10.0.0.1";
        let result = anonymizer.anonymize(input);
        
        // All emails and IPs should be anonymized
        assert!(!result.contains("user1@example.com"));
        assert!(!result.contains("user2@test.org"));
        assert!(!result.contains("10.0.0.1"));
    }
}
