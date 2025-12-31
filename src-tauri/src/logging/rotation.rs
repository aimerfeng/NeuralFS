//! Log rotation functionality
//!
//! Implements log rotation to prevent disk space exhaustion (Requirement 24.2)

use super::LoggingError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use chrono::{DateTime, Utc, Duration};

/// Log rotation strategy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RotationStrategy {
    /// Rotate logs daily
    Daily,
    /// Rotate logs hourly
    Hourly,
    /// Rotate logs when they reach a certain size (in bytes)
    Size(u64),
    /// Never rotate logs
    Never,
}

impl Default for RotationStrategy {
    fn default() -> Self {
        RotationStrategy::Daily
    }
}

/// Log rotation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationConfig {
    /// Rotation strategy
    pub strategy: RotationStrategy,

    /// Maximum number of log files to keep
    pub max_files: usize,

    /// Maximum total size of all log files (in bytes)
    pub max_total_size: u64,

    /// Maximum age of log files (in days)
    pub max_age_days: u32,

    /// Compress rotated logs
    pub compress: bool,

    /// Log directory for rotation management
    pub log_directory: Option<PathBuf>,
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            strategy: RotationStrategy::Daily,
            max_files: 7,
            max_total_size: 100 * 1024 * 1024, // 100 MB
            max_age_days: 30,
            compress: false,
            log_directory: None,
        }
    }
}

impl RotationConfig {
    /// Create a production-ready rotation configuration
    pub fn production() -> Self {
        Self {
            strategy: RotationStrategy::Daily,
            max_files: 14,
            max_total_size: 500 * 1024 * 1024, // 500 MB
            max_age_days: 30,
            compress: true,
            log_directory: None,
        }
    }

    /// Create a minimal rotation configuration for development
    pub fn development() -> Self {
        Self {
            strategy: RotationStrategy::Never,
            max_files: 3,
            max_total_size: 50 * 1024 * 1024, // 50 MB
            max_age_days: 7,
            compress: false,
            log_directory: None,
        }
    }
}

/// Log file information
#[derive(Debug, Clone)]
pub struct LogFileInfo {
    pub path: PathBuf,
    pub size: u64,
    pub modified: DateTime<Utc>,
}

/// Log rotator implementation
pub struct LogRotator {
    config: RotationConfig,
    last_rotation: Option<DateTime<Utc>>,
}

impl LogRotator {
    /// Create a new log rotator with the given configuration
    pub fn new(config: RotationConfig) -> Self {
        Self {
            config,
            last_rotation: None,
        }
    }

    /// Perform log rotation
    pub fn rotate(&mut self) -> Result<(), LoggingError> {
        let log_dir = match &self.config.log_directory {
            Some(dir) => dir.clone(),
            None => {
                if let Some(data_dir) = dirs::data_local_dir() {
                    data_dir.join("NeuralFS").join("logs")
                } else {
                    PathBuf::from("logs")
                }
            }
        };

        if !log_dir.exists() {
            return Ok(()); // Nothing to rotate
        }

        // Get all log files
        let log_files = self.get_log_files(&log_dir)?;

        // Apply rotation policies
        self.cleanup_by_count(&log_files)?;
        self.cleanup_by_size(&log_files)?;
        self.cleanup_by_age(&log_files)?;

        self.last_rotation = Some(Utc::now());

        Ok(())
    }

    /// Get all log files in the directory
    fn get_log_files(&self, log_dir: &PathBuf) -> Result<Vec<LogFileInfo>, LoggingError> {
        let mut files = Vec::new();

        let entries = fs::read_dir(log_dir)?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "log" || ext == "gz" {
                        if let Ok(metadata) = fs::metadata(&path) {
                            let modified = metadata
                                .modified()
                                .map(|t| DateTime::<Utc>::from(t))
                                .unwrap_or_else(|_| Utc::now());

                            files.push(LogFileInfo {
                                path,
                                size: metadata.len(),
                                modified,
                            });
                        }
                    }
                }
            }
        }

        // Sort by modification time (oldest first)
        files.sort_by(|a, b| a.modified.cmp(&b.modified));

        Ok(files)
    }

    /// Cleanup log files by count
    fn cleanup_by_count(&self, files: &[LogFileInfo]) -> Result<(), LoggingError> {
        if files.len() > self.config.max_files {
            let to_remove = files.len() - self.config.max_files;
            for file in files.iter().take(to_remove) {
                tracing::debug!("Removing old log file: {:?}", file.path);
                fs::remove_file(&file.path)?;
            }
        }
        Ok(())
    }

    /// Cleanup log files by total size
    fn cleanup_by_size(&self, files: &[LogFileInfo]) -> Result<(), LoggingError> {
        let total_size: u64 = files.iter().map(|f| f.size).sum();

        if total_size > self.config.max_total_size {
            let mut current_size = total_size;
            for file in files.iter() {
                if current_size <= self.config.max_total_size {
                    break;
                }
                tracing::debug!(
                    "Removing log file to reduce total size: {:?}",
                    file.path
                );
                fs::remove_file(&file.path)?;
                current_size -= file.size;
            }
        }
        Ok(())
    }

    /// Cleanup log files by age
    fn cleanup_by_age(&self, files: &[LogFileInfo]) -> Result<(), LoggingError> {
        let max_age = Duration::days(self.config.max_age_days as i64);
        let cutoff = Utc::now() - max_age;

        for file in files.iter() {
            if file.modified < cutoff {
                tracing::debug!("Removing expired log file: {:?}", file.path);
                fs::remove_file(&file.path)?;
            }
        }
        Ok(())
    }

    /// Check if rotation is needed based on strategy
    pub fn needs_rotation(&self) -> bool {
        match self.config.strategy {
            RotationStrategy::Never => false,
            RotationStrategy::Daily => {
                if let Some(last) = self.last_rotation {
                    Utc::now().date_naive() != last.date_naive()
                } else {
                    true
                }
            }
            RotationStrategy::Hourly => {
                if let Some(last) = self.last_rotation {
                    let now = Utc::now();
                    now.date_naive() != last.date_naive()
                        || now.hour() != last.hour()
                } else {
                    true
                }
            }
            RotationStrategy::Size(_) => {
                // Size-based rotation is handled by tracing-appender
                false
            }
        }
    }

    /// Get the last rotation time
    pub fn last_rotation(&self) -> Option<DateTime<Utc>> {
        self.last_rotation
    }

    /// Get current configuration
    pub fn config(&self) -> &RotationConfig {
        &self.config
    }
}

use chrono::Timelike;
