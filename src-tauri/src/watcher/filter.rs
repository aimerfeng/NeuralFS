//! Directory Filter Module
//!
//! Provides blacklist/whitelist filtering for file system paths.
//! Protects against "folder bombs" (directories with excessive files).

use std::path::Path;
use serde::{Deserialize, Serialize};
use glob::Pattern;

use crate::core::error::{NeuralFSError, Result};

/// Configuration for directory filtering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryFilterConfig {
    /// Blacklist patterns (glob format)
    pub blacklist_patterns: Vec<String>,
    /// Whitelist patterns (higher priority than blacklist)
    pub whitelist_patterns: Vec<String>,
    /// Maximum directory depth
    pub max_depth: u32,
    /// Maximum files per directory (skip if exceeded)
    pub max_files_per_dir: u32,
    /// Maximum file size in bytes
    pub max_file_size: u64,
    /// Whether to follow symbolic links
    pub follow_symlinks: bool,
}

impl Default for DirectoryFilterConfig {
    fn default() -> Self {
        Self {
            blacklist_patterns: vec![
                // Development directories
                "**/node_modules/**".to_string(),
                "**/.git/**".to_string(),
                "**/target/**".to_string(),
                "**/.idea/**".to_string(),
                "**/.vscode/**".to_string(),
                "**/vendor/**".to_string(),
                "**/__pycache__/**".to_string(),
                "**/.venv/**".to_string(),
                "**/venv/**".to_string(),
                "**/dist/**".to_string(),
                "**/build/**".to_string(),
                "**/.cache/**".to_string(),
                "**/bower_components/**".to_string(),
                "**/.npm/**".to_string(),
                "**/.yarn/**".to_string(),
                
                // System directories (Windows)
                "**/System Volume Information/**".to_string(),
                "**/$Recycle.Bin/**".to_string(),
                "**/Windows/**".to_string(),
                "**/Program Files/**".to_string(),
                "**/Program Files (x86)/**".to_string(),
                "**/ProgramData/**".to_string(),
                
                // System directories (Unix)
                "**/proc/**".to_string(),
                "**/sys/**".to_string(),
                "**/dev/**".to_string(),
                
                // Temporary files
                "**/*.tmp".to_string(),
                "**/*.temp".to_string(),
                "**/*.swp".to_string(),
                "**/*~".to_string(),
                "**/.DS_Store".to_string(),
                "**/Thumbs.db".to_string(),
                "**/*.log".to_string(),
                
                // Lock files
                "**/*.lock".to_string(),
                "**/package-lock.json".to_string(),
                "**/yarn.lock".to_string(),
                "**/Cargo.lock".to_string(),
            ],
            whitelist_patterns: vec![],
            max_depth: 20,
            max_files_per_dir: 10000,
            max_file_size: 500 * 1024 * 1024, // 500MB
            follow_symlinks: false,
        }
    }
}


/// Result of filtering a path
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterResult {
    /// Path should be included
    Include,
    /// Path should be excluded
    Exclude(FilterReason),
}

/// Reason for excluding a path
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterReason {
    /// Path matches a blacklist pattern
    Blacklisted,
    /// Directory depth exceeds maximum
    TooDeep,
    /// Directory contains too many files
    TooManyFiles,
    /// File is too large
    TooLarge,
    /// Path is a symbolic link (and follow_symlinks is false)
    Symlink,
}

/// User-configurable filter rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserFilterRules {
    /// User-added blacklist patterns
    pub custom_blacklist: Vec<String>,
    /// User-added whitelist patterns
    pub custom_whitelist: Vec<String>,
    /// Whether to use default blacklist
    pub use_default_blacklist: bool,
}

impl Default for UserFilterRules {
    fn default() -> Self {
        Self {
            custom_blacklist: vec![],
            custom_whitelist: vec![],
            use_default_blacklist: true,
        }
    }
}

/// Directory filter with blacklist/whitelist support
pub struct DirectoryFilter {
    config: DirectoryFilterConfig,
    blacklist_matchers: Vec<Pattern>,
    whitelist_matchers: Vec<Pattern>,
}

impl DirectoryFilter {
    /// Create a new DirectoryFilter with the given configuration
    pub fn new(config: DirectoryFilterConfig) -> Result<Self> {
        let blacklist_matchers = config
            .blacklist_patterns
            .iter()
            .map(|p| Pattern::new(p))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| NeuralFSError::ConfigError(format!("Invalid blacklist pattern: {}", e)))?;

        let whitelist_matchers = config
            .whitelist_patterns
            .iter()
            .map(|p| Pattern::new(p))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| NeuralFSError::ConfigError(format!("Invalid whitelist pattern: {}", e)))?;

        Ok(Self {
            config,
            blacklist_matchers,
            whitelist_matchers,
        })
    }

    /// Create a DirectoryFilter with default configuration
    pub fn with_defaults() -> Result<Self> {
        Self::new(DirectoryFilterConfig::default())
    }

    /// Create a DirectoryFilter by merging default config with user rules
    pub fn with_user_rules(user_rules: UserFilterRules) -> Result<Self> {
        let mut config = if user_rules.use_default_blacklist {
            DirectoryFilterConfig::default()
        } else {
            DirectoryFilterConfig {
                blacklist_patterns: vec![],
                ..DirectoryFilterConfig::default()
            }
        };

        // Add user custom patterns
        config.blacklist_patterns.extend(user_rules.custom_blacklist);
        config.whitelist_patterns.extend(user_rules.custom_whitelist);

        Self::new(config)
    }


    /// Check if a path should be filtered
    pub fn should_filter(&self, path: &Path) -> FilterResult {
        let path_str = path.to_string_lossy();
        // Normalize path separators for cross-platform matching
        let normalized_path = path_str.replace('\\', "/");

        // 1. Check whitelist first (higher priority)
        for matcher in &self.whitelist_matchers {
            if matcher.matches(&normalized_path) {
                return FilterResult::Include;
            }
        }

        // 2. Check blacklist
        for matcher in &self.blacklist_matchers {
            if matcher.matches(&normalized_path) {
                return FilterResult::Exclude(FilterReason::Blacklisted);
            }
        }

        // 3. Check depth
        let depth = path.components().count();
        if depth > self.config.max_depth as usize {
            return FilterResult::Exclude(FilterReason::TooDeep);
        }

        FilterResult::Include
    }

    /// Check if a path matches any blacklist pattern
    pub fn is_blacklisted(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        let normalized_path = path_str.replace('\\', "/");

        for matcher in &self.blacklist_matchers {
            if matcher.matches(&normalized_path) {
                return true;
            }
        }
        false
    }

    /// Check if a path matches any whitelist pattern
    pub fn is_whitelisted(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        let normalized_path = path_str.replace('\\', "/");

        for matcher in &self.whitelist_matchers {
            if matcher.matches(&normalized_path) {
                return true;
            }
        }
        false
    }

    /// Check if a directory should be skipped due to too many files
    pub fn should_skip_directory(&self, file_count: u32) -> FilterResult {
        if file_count > self.config.max_files_per_dir {
            return FilterResult::Exclude(FilterReason::TooManyFiles);
        }
        FilterResult::Include
    }

    /// Async version: Check directory file count and determine if it should be skipped
    pub async fn should_skip_directory_async(&self, path: &Path) -> FilterResult {
        let count = self.count_files_fast(path).await;
        self.should_skip_directory(count)
    }

    /// Fast file count in a directory (non-recursive)
    async fn count_files_fast(&self, path: &Path) -> u32 {
        let mut count = 0u32;

        if let Ok(mut entries) = tokio::fs::read_dir(path).await {
            while let Ok(Some(_)) = entries.next_entry().await {
                count += 1;
                // Early exit to avoid wasting time in huge directories
                if count > self.config.max_files_per_dir {
                    break;
                }
            }
        }

        count
    }

    /// Synchronous version of count_files_fast
    pub fn count_files_fast_sync(&self, path: &Path) -> u32 {
        let mut count = 0u32;

        if let Ok(entries) = std::fs::read_dir(path) {
            for _ in entries {
                count += 1;
                if count > self.config.max_files_per_dir {
                    break;
                }
            }
        }

        count
    }

    /// Check file size
    pub fn check_file_size(&self, size: u64) -> FilterResult {
        if size > self.config.max_file_size {
            return FilterResult::Exclude(FilterReason::TooLarge);
        }
        FilterResult::Include
    }

    /// Get the configuration
    pub fn config(&self) -> &DirectoryFilterConfig {
        &self.config
    }

    /// Get max files per directory limit
    pub fn max_files_per_dir(&self) -> u32 {
        self.config.max_files_per_dir
    }

    /// Get max depth limit
    pub fn max_depth(&self) -> u32 {
        self.config.max_depth
    }

    /// Check if symlinks should be followed
    pub fn follow_symlinks(&self) -> bool {
        self.config.follow_symlinks
    }

    /// Add a blacklist pattern dynamically
    pub fn add_blacklist_pattern(&mut self, pattern: &str) -> Result<()> {
        let matcher = Pattern::new(pattern)
            .map_err(|e| NeuralFSError::ConfigError(format!("Invalid pattern: {}", e)))?;
        self.blacklist_matchers.push(matcher);
        self.config.blacklist_patterns.push(pattern.to_string());
        Ok(())
    }

    /// Add a whitelist pattern dynamically
    pub fn add_whitelist_pattern(&mut self, pattern: &str) -> Result<()> {
        let matcher = Pattern::new(pattern)
            .map_err(|e| NeuralFSError::ConfigError(format!("Invalid pattern: {}", e)))?;
        self.whitelist_matchers.push(matcher);
        self.config.whitelist_patterns.push(pattern.to_string());
        Ok(())
    }

    /// Remove a blacklist pattern
    pub fn remove_blacklist_pattern(&mut self, pattern: &str) {
        self.config.blacklist_patterns.retain(|p| p != pattern);
        // Rebuild matchers
        self.blacklist_matchers = self.config.blacklist_patterns
            .iter()
            .filter_map(|p| Pattern::new(p).ok())
            .collect();
    }

    /// Remove a whitelist pattern
    pub fn remove_whitelist_pattern(&mut self, pattern: &str) {
        self.config.whitelist_patterns.retain(|p| p != pattern);
        // Rebuild matchers
        self.whitelist_matchers = self.config.whitelist_patterns
            .iter()
            .filter_map(|p| Pattern::new(p).ok())
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_default_blacklist_node_modules() {
        let filter = DirectoryFilter::with_defaults().unwrap();
        let path = PathBuf::from("/home/user/project/node_modules/package/index.js");
        assert!(matches!(filter.should_filter(&path), FilterResult::Exclude(FilterReason::Blacklisted)));
    }

    #[test]
    fn test_default_blacklist_git() {
        let filter = DirectoryFilter::with_defaults().unwrap();
        let path = PathBuf::from("/home/user/project/.git/objects/abc123");
        assert!(matches!(filter.should_filter(&path), FilterResult::Exclude(FilterReason::Blacklisted)));
    }

    #[test]
    fn test_whitelist_priority() {
        let config = DirectoryFilterConfig {
            blacklist_patterns: vec!["**/secret/**".to_string()],
            whitelist_patterns: vec!["**/secret/important/**".to_string()],
            ..Default::default()
        };
        let filter = DirectoryFilter::new(config).unwrap();
        
        // Blacklisted path
        let blocked = PathBuf::from("/home/user/secret/hidden.txt");
        assert!(matches!(filter.should_filter(&blocked), FilterResult::Exclude(FilterReason::Blacklisted)));
        
        // Whitelisted path (should be included despite blacklist)
        let allowed = PathBuf::from("/home/user/secret/important/file.txt");
        assert!(matches!(filter.should_filter(&allowed), FilterResult::Include));
    }

    #[test]
    fn test_normal_path_included() {
        let filter = DirectoryFilter::with_defaults().unwrap();
        let path = PathBuf::from("/home/user/documents/report.pdf");
        assert!(matches!(filter.should_filter(&path), FilterResult::Include));
    }

    #[test]
    fn test_depth_limit() {
        let config = DirectoryFilterConfig {
            max_depth: 3,
            ..Default::default()
        };
        let filter = DirectoryFilter::new(config).unwrap();
        
        // Within depth limit
        let shallow = PathBuf::from("/a/b/c");
        assert!(matches!(filter.should_filter(&shallow), FilterResult::Include));
        
        // Exceeds depth limit
        let deep = PathBuf::from("/a/b/c/d/e/f");
        assert!(matches!(filter.should_filter(&deep), FilterResult::Exclude(FilterReason::TooDeep)));
    }

    #[test]
    fn test_file_size_check() {
        let config = DirectoryFilterConfig {
            max_file_size: 1024 * 1024, // 1MB
            ..Default::default()
        };
        let filter = DirectoryFilter::new(config).unwrap();
        
        // Small file
        assert!(matches!(filter.check_file_size(1024), FilterResult::Include));
        
        // Large file
        assert!(matches!(filter.check_file_size(2 * 1024 * 1024), FilterResult::Exclude(FilterReason::TooLarge)));
    }

    #[test]
    fn test_directory_file_count() {
        let config = DirectoryFilterConfig {
            max_files_per_dir: 100,
            ..Default::default()
        };
        let filter = DirectoryFilter::new(config).unwrap();
        
        // Within limit
        assert!(matches!(filter.should_skip_directory(50), FilterResult::Include));
        
        // Exceeds limit
        assert!(matches!(filter.should_skip_directory(150), FilterResult::Exclude(FilterReason::TooManyFiles)));
    }

    #[test]
    fn test_windows_path_normalization() {
        let filter = DirectoryFilter::with_defaults().unwrap();
        let path = PathBuf::from("C:\\Users\\user\\project\\node_modules\\package");
        assert!(matches!(filter.should_filter(&path), FilterResult::Exclude(FilterReason::Blacklisted)));
    }

    #[test]
    fn test_user_rules() {
        let user_rules = UserFilterRules {
            custom_blacklist: vec!["**/my_secret/**".to_string()],
            custom_whitelist: vec!["**/my_secret/public/**".to_string()],
            use_default_blacklist: true,
        };
        let filter = DirectoryFilter::with_user_rules(user_rules).unwrap();
        
        // Custom blacklist works
        let blocked = PathBuf::from("/home/user/my_secret/file.txt");
        assert!(matches!(filter.should_filter(&blocked), FilterResult::Exclude(FilterReason::Blacklisted)));
        
        // Custom whitelist works
        let allowed = PathBuf::from("/home/user/my_secret/public/file.txt");
        assert!(matches!(filter.should_filter(&allowed), FilterResult::Include));
        
        // Default blacklist still works
        let node_modules = PathBuf::from("/home/user/project/node_modules/pkg");
        assert!(matches!(filter.should_filter(&node_modules), FilterResult::Exclude(FilterReason::Blacklisted)));
    }

    #[test]
    fn test_dynamic_pattern_add() {
        let mut filter = DirectoryFilter::with_defaults().unwrap();
        
        let path = PathBuf::from("/home/user/custom_blocked/file.txt");
        assert!(matches!(filter.should_filter(&path), FilterResult::Include));
        
        filter.add_blacklist_pattern("**/custom_blocked/**").unwrap();
        assert!(matches!(filter.should_filter(&path), FilterResult::Exclude(FilterReason::Blacklisted)));
    }
}
