//! Log export functionality
//!
//! Implements log export for bug reports (Requirement 24.5)

use super::LoggingError;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use chrono::{DateTime, Utc, Duration};

/// Export format for logs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    /// Plain text format
    #[default]
    Text,
    /// JSON format
    Json,
    /// Compressed archive (zip)
    Zip,
}

/// Log export configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportConfig {
    /// Default export format
    pub format: ExportFormat,

    /// Include system information in export
    pub include_system_info: bool,

    /// Include configuration in export
    pub include_config: bool,

    /// Maximum number of log lines to export
    pub max_lines: usize,

    /// Maximum age of logs to include (in hours)
    pub max_age_hours: u32,

    /// Anonymize sensitive data in export
    pub anonymize: bool,

    /// Log directory to export from
    pub log_directory: Option<PathBuf>,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            format: ExportFormat::Text,
            include_system_info: true,
            include_config: false,
            max_lines: 10000,
            max_age_hours: 24,
            anonymize: true,
            log_directory: None,
        }
    }
}

/// Result of a log export operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResult {
    /// Path to the exported file
    pub output_path: PathBuf,

    /// Number of log lines exported
    pub lines_exported: usize,

    /// Number of files included
    pub files_included: usize,

    /// Total size of export (bytes)
    pub total_size: u64,

    /// Export timestamp
    pub exported_at: DateTime<Utc>,

    /// Whether data was anonymized
    pub anonymized: bool,
}

/// Log exporter implementation
pub struct LogExporter {
    config: ExportConfig,
}

impl LogExporter {
    /// Create a new log exporter with the given configuration
    pub fn new(config: ExportConfig) -> Self {
        Self { config }
    }

    /// Export logs to the specified output path
    pub async fn export(&self, output_path: PathBuf) -> Result<ExportResult, LoggingError> {
        let log_dir = self.get_log_directory();

        if !log_dir.exists() {
            return Err(LoggingError::ExportError(format!(
                "Log directory does not exist: {:?}",
                log_dir
            )));
        }

        // Collect log files
        let log_files = self.collect_log_files(&log_dir)?;

        // Create output file
        let mut output_file = File::create(&output_path)?;

        let mut lines_exported = 0;
        let files_included = log_files.len();

        // Write header
        if self.config.include_system_info {
            self.write_system_info(&mut output_file)?;
        }

        // Write log content
        for log_file in &log_files {
            lines_exported += self.export_file(&mut output_file, log_file)?;

            if lines_exported >= self.config.max_lines {
                break;
            }
        }

        // Write footer
        writeln!(output_file, "\n--- End of Export ---")?;
        writeln!(output_file, "Exported at: {}", Utc::now().to_rfc3339())?;

        let total_size = fs::metadata(&output_path)?.len();

        Ok(ExportResult {
            output_path,
            lines_exported,
            files_included,
            total_size,
            exported_at: Utc::now(),
            anonymized: self.config.anonymize,
        })
    }

    /// Get the log directory
    fn get_log_directory(&self) -> PathBuf {
        self.config.log_directory.clone().unwrap_or_else(|| {
            if let Some(data_dir) = dirs::data_local_dir() {
                data_dir.join("NeuralFS").join("logs")
            } else {
                PathBuf::from("logs")
            }
        })
    }

    /// Collect log files within the age limit
    fn collect_log_files(&self, log_dir: &PathBuf) -> Result<Vec<PathBuf>, LoggingError> {
        let mut files = Vec::new();
        let cutoff = Utc::now() - Duration::hours(self.config.max_age_hours as i64);

        let entries = fs::read_dir(log_dir)?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "log" {
                        if let Ok(metadata) = fs::metadata(&path) {
                            let modified = metadata
                                .modified()
                                .map(|t| DateTime::<Utc>::from(t))
                                .unwrap_or_else(|_| Utc::now());

                            if modified >= cutoff {
                                files.push(path);
                            }
                        }
                    }
                }
            }
        }

        // Sort by modification time (newest first)
        files.sort_by(|a, b| {
            let a_time = fs::metadata(a)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            let b_time = fs::metadata(b)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            b_time.cmp(&a_time)
        });

        Ok(files)
    }

    /// Write system information to the export
    fn write_system_info(&self, output: &mut File) -> Result<(), LoggingError> {
        writeln!(output, "=== NeuralFS Log Export ===")?;
        writeln!(output, "Export Time: {}", Utc::now().to_rfc3339())?;
        writeln!(output, "OS: {}", std::env::consts::OS)?;
        writeln!(output, "Architecture: {}", std::env::consts::ARCH)?;
        writeln!(output, "Version: {}", env!("CARGO_PKG_VERSION"))?;
        writeln!(output, "")?;
        writeln!(output, "=== Log Content ===")?;
        writeln!(output, "")?;
        Ok(())
    }

    /// Export a single log file
    fn export_file(&self, output: &mut File, log_file: &PathBuf) -> Result<usize, LoggingError> {
        let file = File::open(log_file)?;
        let reader = BufReader::new(file);
        let mut lines_written = 0;

        writeln!(output, "--- File: {:?} ---", log_file.file_name().unwrap_or_default())?;

        for line in reader.lines() {
            let line = line?;
            let processed_line = if self.config.anonymize {
                self.anonymize_line(&line)
            } else {
                line
            };
            writeln!(output, "{}", processed_line)?;
            lines_written += 1;

            if lines_written >= self.config.max_lines {
                break;
            }
        }

        writeln!(output, "")?;
        Ok(lines_written)
    }

    /// Anonymize sensitive data in a log line
    fn anonymize_line(&self, line: &str) -> String {
        let mut result = line.to_string();

        // Anonymize usernames in paths
        if let Ok(username) = std::env::var("USERNAME") {
            result = result.replace(&username, "[USER]");
        }
        if let Ok(username) = std::env::var("USER") {
            result = result.replace(&username, "[USER]");
        }

        // Anonymize home directory
        if let Some(home) = dirs::home_dir() {
            if let Some(home_str) = home.to_str() {
                result = result.replace(home_str, "[HOME]");
            }
        }

        // Anonymize potential API keys (simple pattern matching)
        let api_key_pattern = regex::Regex::new(r"(api[_-]?key|token|secret)[=:]\s*['\"]?([a-zA-Z0-9_-]{20,})['\"]?")
            .unwrap();
        result = api_key_pattern.replace_all(&result, "$1=[REDACTED]").to_string();

        // Anonymize email addresses
        let email_pattern = regex::Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}")
            .unwrap();
        result = email_pattern.replace_all(&result, "[EMAIL]").to_string();

        // Anonymize IP addresses
        let ip_pattern = regex::Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b")
            .unwrap();
        result = ip_pattern.replace_all(&result, "[IP]").to_string();

        result
    }

    /// Get current configuration
    pub fn config(&self) -> &ExportConfig {
        &self.config
    }
}

/// Quick export function for bug reports
pub async fn export_for_bug_report(output_path: PathBuf) -> Result<ExportResult, LoggingError> {
    let config = ExportConfig {
        format: ExportFormat::Text,
        include_system_info: true,
        include_config: false,
        max_lines: 5000,
        max_age_hours: 48,
        anonymize: true,
        log_directory: None,
    };

    let exporter = LogExporter::new(config);
    exporter.export(output_path).await
}
