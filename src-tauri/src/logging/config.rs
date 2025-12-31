//! Logging configuration types
//!
//! Provides configuration structures for the logging system.

use super::rotation::RotationConfig;
use super::export::ExportConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Log verbosity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl LogLevel {
    /// Convert to tracing level
    pub fn to_tracing_level(&self) -> tracing::Level {
        match self {
            LogLevel::Trace => tracing::Level::TRACE,
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Warn => tracing::Level::WARN,
            LogLevel::Error => tracing::Level::ERROR,
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Trace => write!(f, "trace"),
            LogLevel::Debug => write!(f, "debug"),
            LogLevel::Info => write!(f, "info"),
            LogLevel::Warn => write!(f, "warn"),
            LogLevel::Error => write!(f, "error"),
        }
    }
}

/// Log output format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// Human-readable text format
    #[default]
    Text,
    /// Structured JSON format for machine parsing
    Json,
}

/// Log output destination
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogOutput {
    /// Output to console only
    Console,
    /// Output to file only
    File,
    /// Output to both console and file
    #[default]
    Both,
}

/// Main logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Global log level
    pub level: LogLevel,

    /// Log output format
    pub format: LogFormat,

    /// Log output destination
    pub output: LogOutput,

    /// Directory for log files (if file output is enabled)
    pub log_directory: Option<PathBuf>,

    /// Module-specific log levels
    #[serde(default)]
    pub module_levels: HashMap<String, LogLevel>,

    /// Include target (module path) in log output
    #[serde(default = "default_true")]
    pub include_target: bool,

    /// Include thread ID in log output
    #[serde(default)]
    pub include_thread_id: bool,

    /// Include file and line number in log output
    #[serde(default)]
    pub include_file_info: bool,

    /// Log rotation configuration
    #[serde(default)]
    pub rotation: RotationConfig,

    /// Log export configuration
    #[serde(default)]
    pub export: ExportConfig,

    /// Enable performance metrics logging
    #[serde(default = "default_true")]
    pub enable_metrics: bool,

    /// Performance metrics sampling rate (0.0 - 1.0)
    #[serde(default = "default_sampling_rate")]
    pub metrics_sampling_rate: f64,
}

fn default_true() -> bool {
    true
}

fn default_sampling_rate() -> f64 {
    1.0
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::Info,
            format: LogFormat::Text,
            output: LogOutput::Both,
            log_directory: Some(default_log_directory()),
            module_levels: HashMap::new(),
            include_target: true,
            include_thread_id: false,
            include_file_info: false,
            rotation: RotationConfig::default(),
            export: ExportConfig::default(),
            enable_metrics: true,
            metrics_sampling_rate: 1.0,
        }
    }
}

impl LoggingConfig {
    /// Create a new logging configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the global log level
    pub fn with_level(mut self, level: LogLevel) -> Self {
        self.level = level;
        self
    }

    /// Set the log format
    pub fn with_format(mut self, format: LogFormat) -> Self {
        self.format = format;
        self
    }

    /// Set the log output destination
    pub fn with_output(mut self, output: LogOutput) -> Self {
        self.output = output;
        self
    }

    /// Set the log directory
    pub fn with_log_directory(mut self, dir: PathBuf) -> Self {
        self.log_directory = Some(dir);
        self
    }

    /// Add a module-specific log level
    pub fn with_module_level(mut self, module: impl Into<String>, level: LogLevel) -> Self {
        self.module_levels.insert(module.into(), level);
        self
    }

    /// Enable or disable target in log output
    pub fn with_target(mut self, include: bool) -> Self {
        self.include_target = include;
        self
    }

    /// Enable or disable thread ID in log output
    pub fn with_thread_id(mut self, include: bool) -> Self {
        self.include_thread_id = include;
        self
    }

    /// Enable or disable file info in log output
    pub fn with_file_info(mut self, include: bool) -> Self {
        self.include_file_info = include;
        self
    }

    /// Set the rotation configuration
    pub fn with_rotation(mut self, rotation: RotationConfig) -> Self {
        self.rotation = rotation;
        self
    }

    /// Set the export configuration
    pub fn with_export(mut self, export: ExportConfig) -> Self {
        self.export = export;
        self
    }

    /// Create a development configuration with verbose logging
    pub fn development() -> Self {
        Self {
            level: LogLevel::Debug,
            format: LogFormat::Text,
            output: LogOutput::Console,
            log_directory: None,
            module_levels: HashMap::new(),
            include_target: true,
            include_thread_id: true,
            include_file_info: true,
            rotation: RotationConfig::default(),
            export: ExportConfig::default(),
            enable_metrics: true,
            metrics_sampling_rate: 1.0,
        }
    }

    /// Create a production configuration with structured logging
    pub fn production() -> Self {
        Self {
            level: LogLevel::Info,
            format: LogFormat::Json,
            output: LogOutput::Both,
            log_directory: Some(default_log_directory()),
            module_levels: HashMap::new(),
            include_target: true,
            include_thread_id: false,
            include_file_info: false,
            rotation: RotationConfig::production(),
            export: ExportConfig::default(),
            enable_metrics: true,
            metrics_sampling_rate: 0.1, // Sample 10% in production
        }
    }
}

/// Get the default log directory based on the platform
fn default_log_directory() -> PathBuf {
    if let Some(data_dir) = dirs::data_local_dir() {
        data_dir.join("NeuralFS").join("logs")
    } else {
        PathBuf::from("logs")
    }
}
