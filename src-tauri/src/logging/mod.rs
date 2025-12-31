//! Logging System for NeuralFS
//!
//! Provides comprehensive logging with:
//! - Structured logs with configurable verbosity levels (Requirement 24.1)
//! - Log rotation to prevent disk space exhaustion (Requirement 24.2)
//! - Log export for bug reports (Requirement 24.5)
//! - Performance metrics for bottleneck identification (Requirement 24.7)

mod config;
mod rotation;
mod export;
mod metrics;

#[cfg(test)]
mod tests;

pub use config::{LoggingConfig, LogLevel, LogFormat, LogOutput};
pub use rotation::{LogRotator, RotationConfig, RotationStrategy};
pub use export::{LogExporter, ExportConfig, ExportFormat, ExportResult};
pub use metrics::{PerformanceMetrics, MetricEntry, MetricType, MetricsCollector};

use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::RwLock;
use thiserror::Error;
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter, Layer,
};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};

/// Logging system errors
#[derive(Debug, Error)]
pub enum LoggingError {
    #[error("Failed to initialize logging: {0}")]
    InitializationError(String),

    #[error("Failed to create log directory: {0}")]
    DirectoryCreationError(String),

    #[error("Failed to export logs: {0}")]
    ExportError(String),

    #[error("Failed to rotate logs: {0}")]
    RotationError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Result type for logging operations
pub type LoggingResult<T> = Result<T, LoggingError>;

/// Global logging system state
pub struct LoggingSystem {
    config: LoggingConfig,
    rotator: Arc<RwLock<LogRotator>>,
    exporter: LogExporter,
    metrics: Arc<MetricsCollector>,
    _guards: Vec<WorkerGuard>,
}

impl LoggingSystem {
    /// Initialize the logging system with the given configuration
    pub fn init(config: LoggingConfig) -> LoggingResult<Self> {
        // Ensure log directory exists
        if let Some(ref log_dir) = config.log_directory {
            std::fs::create_dir_all(log_dir).map_err(|e| {
                LoggingError::DirectoryCreationError(format!(
                    "Failed to create log directory {:?}: {}",
                    log_dir, e
                ))
            })?;
        }

        let mut guards = Vec::new();

        // Build the env filter based on configuration
        let env_filter = Self::build_env_filter(&config);

        // Create layers based on output configuration
        let registry = tracing_subscriber::registry();

        match config.output {
            LogOutput::Console => {
                let fmt_layer = Self::create_console_layer(&config);
                registry
                    .with(env_filter)
                    .with(fmt_layer)
                    .try_init()
                    .map_err(|e| LoggingError::InitializationError(e.to_string()))?;
            }
            LogOutput::File => {
                let (file_layer, guard) = Self::create_file_layer(&config)?;
                guards.push(guard);
                registry
                    .with(env_filter)
                    .with(file_layer)
                    .try_init()
                    .map_err(|e| LoggingError::InitializationError(e.to_string()))?;
            }
            LogOutput::Both => {
                let console_layer = Self::create_console_layer(&config);
                let (file_layer, guard) = Self::create_file_layer(&config)?;
                guards.push(guard);
                registry
                    .with(env_filter)
                    .with(console_layer)
                    .with(file_layer)
                    .try_init()
                    .map_err(|e| LoggingError::InitializationError(e.to_string()))?;
            }
        }

        // Initialize rotator
        let rotation_config = config.rotation.clone();
        let rotator = Arc::new(RwLock::new(LogRotator::new(rotation_config)));

        // Initialize exporter
        let export_config = config.export.clone();
        let exporter = LogExporter::new(export_config);

        // Initialize metrics collector
        let metrics = Arc::new(MetricsCollector::new());

        Ok(Self {
            config,
            rotator,
            exporter,
            metrics,
            _guards: guards,
        })
    }

    /// Build environment filter from configuration
    fn build_env_filter(config: &LoggingConfig) -> EnvFilter {
        let level_str = match config.level {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        };

        // Start with the base level
        let mut filter = EnvFilter::new(level_str);

        // Add module-specific filters
        for (module, level) in &config.module_levels {
            let module_level = match level {
                LogLevel::Trace => "trace",
                LogLevel::Debug => "debug",
                LogLevel::Info => "info",
                LogLevel::Warn => "warn",
                LogLevel::Error => "error",
            };
            filter = filter.add_directive(
                format!("{}={}", module, module_level)
                    .parse()
                    .unwrap_or_else(|_| tracing::Level::INFO.into()),
            );
        }

        filter
    }

    /// Create console logging layer
    fn create_console_layer<S>(config: &LoggingConfig) -> impl Layer<S>
    where
        S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
    {
        let layer = fmt::layer()
            .with_target(config.include_target)
            .with_thread_ids(config.include_thread_id)
            .with_file(config.include_file_info)
            .with_line_number(config.include_file_info);

        if config.format == LogFormat::Json {
            layer.json().boxed()
        } else {
            layer.boxed()
        }
    }

    /// Create file logging layer with rotation
    fn create_file_layer<S>(
        config: &LoggingConfig,
    ) -> LoggingResult<(impl Layer<S>, WorkerGuard)>
    where
        S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
    {
        let log_dir = config
            .log_directory
            .clone()
            .unwrap_or_else(|| PathBuf::from("logs"));

        let rotation = match config.rotation.strategy {
            RotationStrategy::Daily => Rotation::DAILY,
            RotationStrategy::Hourly => Rotation::HOURLY,
            RotationStrategy::Size(_) => Rotation::DAILY, // Size-based uses daily as base
            RotationStrategy::Never => Rotation::NEVER,
        };

        let file_appender = RollingFileAppender::new(rotation, &log_dir, "neuralfs.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        let layer = fmt::layer()
            .with_writer(non_blocking)
            .with_target(config.include_target)
            .with_thread_ids(config.include_thread_id)
            .with_file(config.include_file_info)
            .with_line_number(config.include_file_info)
            .with_ansi(false); // No ANSI colors in file output

        if config.format == LogFormat::Json {
            Ok((layer.json().boxed(), guard))
        } else {
            Ok((layer.boxed(), guard))
        }
    }

    /// Get the metrics collector for recording performance metrics
    pub fn metrics(&self) -> Arc<MetricsCollector> {
        Arc::clone(&self.metrics)
    }

    /// Export logs for bug reports
    pub async fn export_logs(&self, output_path: PathBuf) -> LoggingResult<ExportResult> {
        self.exporter.export(output_path).await
    }

    /// Manually trigger log rotation
    pub fn rotate_logs(&self) -> LoggingResult<()> {
        let mut rotator = self.rotator.write();
        rotator.rotate()
    }

    /// Get current log directory
    pub fn log_directory(&self) -> Option<&PathBuf> {
        self.config.log_directory.as_ref()
    }

    /// Get current log level
    pub fn log_level(&self) -> LogLevel {
        self.config.level
    }
}

/// Initialize logging with default configuration
/// This is a convenience function for simple initialization
pub fn init_default_logging() -> LoggingResult<LoggingSystem> {
    let config = LoggingConfig::default();
    LoggingSystem::init(config)
}

/// Initialize logging with custom configuration
pub fn init_logging(config: LoggingConfig) -> LoggingResult<LoggingSystem> {
    LoggingSystem::init(config)
}

/// Macro for logging with performance timing
#[macro_export]
macro_rules! timed_info {
    ($metrics:expr, $name:expr, $($arg:tt)*) => {{
        let start = std::time::Instant::now();
        let result = { $($arg)* };
        let duration = start.elapsed();
        $metrics.record($name, $crate::logging::MetricType::Duration(duration));
        tracing::info!(
            target: "performance",
            operation = $name,
            duration_ms = duration.as_millis() as u64,
            "Operation completed"
        );
        result
    }};
}

/// Macro for logging with performance timing (debug level)
#[macro_export]
macro_rules! timed_debug {
    ($metrics:expr, $name:expr, $($arg:tt)*) => {{
        let start = std::time::Instant::now();
        let result = { $($arg)* };
        let duration = start.elapsed();
        $metrics.record($name, $crate::logging::MetricType::Duration(duration));
        tracing::debug!(
            target: "performance",
            operation = $name,
            duration_ms = duration.as_millis() as u64,
            "Operation completed"
        );
        result
    }};
}
