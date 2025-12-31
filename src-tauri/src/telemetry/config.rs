//! Telemetry configuration types

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Telemetry endpoint configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEndpoint {
    /// Endpoint URL
    pub url: String,
    /// API key (if required)
    pub api_key: Option<String>,
    /// Request timeout
    pub timeout_ms: u64,
}

impl Default for TelemetryEndpoint {
    fn default() -> Self {
        Self {
            // Default to a placeholder - should be configured by the application
            url: String::new(),
            api_key: None,
            timeout_ms: 5000,
        }
    }
}

/// Main telemetry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    /// Whether telemetry is enabled (master switch)
    pub enabled: bool,

    /// Telemetry endpoint configuration
    pub endpoint: TelemetryEndpoint,

    /// Directory for storing telemetry data
    pub data_directory: PathBuf,

    /// File for storing consent status
    pub consent_file: PathBuf,

    /// Maximum number of events to batch before sending
    pub batch_size: usize,

    /// Maximum time to wait before sending a batch (seconds)
    pub flush_interval_secs: u64,

    /// Maximum number of events to store locally
    pub max_local_events: usize,

    /// Whether to collect performance metrics
    pub collect_performance: bool,

    /// Whether to collect feature usage
    pub collect_feature_usage: bool,

    /// Whether to collect error reports
    pub collect_errors: bool,

    /// Sampling rate for performance events (0.0 - 1.0)
    pub performance_sampling_rate: f64,

    /// Application version for telemetry
    pub app_version: String,

    /// Platform identifier
    pub platform: String,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        let data_dir = if let Some(data_dir) = dirs::data_local_dir() {
            data_dir.join("NeuralFS").join("telemetry")
        } else {
            PathBuf::from("telemetry")
        };

        Self {
            enabled: false, // Disabled by default - requires explicit opt-in
            endpoint: TelemetryEndpoint::default(),
            data_directory: data_dir.clone(),
            consent_file: data_dir.join("consent.json"),
            batch_size: 50,
            flush_interval_secs: 300, // 5 minutes
            max_local_events: 1000,
            collect_performance: true,
            collect_feature_usage: true,
            collect_errors: true,
            performance_sampling_rate: 0.1, // 10% sampling
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            platform: std::env::consts::OS.to_string(),
        }
    }
}

impl TelemetryConfig {
    /// Create a new telemetry configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable telemetry
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set the telemetry endpoint
    pub fn with_endpoint(mut self, url: impl Into<String>) -> Self {
        self.endpoint.url = url.into();
        self
    }

    /// Set the API key
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.endpoint.api_key = Some(api_key.into());
        self
    }

    /// Set the data directory
    pub fn with_data_directory(mut self, dir: PathBuf) -> Self {
        self.data_directory = dir.clone();
        self.consent_file = dir.join("consent.json");
        self
    }

    /// Set the batch size
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    /// Set the flush interval
    pub fn with_flush_interval(mut self, duration: Duration) -> Self {
        self.flush_interval_secs = duration.as_secs();
        self
    }

    /// Set performance sampling rate
    pub fn with_sampling_rate(mut self, rate: f64) -> Self {
        self.performance_sampling_rate = rate.clamp(0.0, 1.0);
        self
    }

    /// Disable performance collection
    pub fn without_performance(mut self) -> Self {
        self.collect_performance = false;
        self
    }

    /// Disable feature usage collection
    pub fn without_feature_usage(mut self) -> Self {
        self.collect_feature_usage = false;
        self
    }

    /// Disable error collection
    pub fn without_errors(mut self) -> Self {
        self.collect_errors = false;
        self
    }

    /// Get flush interval as Duration
    pub fn flush_interval(&self) -> Duration {
        Duration::from_secs(self.flush_interval_secs)
    }

    /// Get endpoint timeout as Duration
    pub fn endpoint_timeout(&self) -> Duration {
        Duration::from_millis(self.endpoint.timeout_ms)
    }

    /// Check if endpoint is configured
    pub fn has_endpoint(&self) -> bool {
        !self.endpoint.url.is_empty()
    }
}
