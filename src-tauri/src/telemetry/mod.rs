//! Telemetry System for NeuralFS
//!
//! Provides optional anonymous usage statistics collection with:
//! - Explicit user consent requirement (Requirement 24.3)
//! - No collection of file content, names, or personal information (Requirement 24.4)
//! - Complete opt-out capability at any time (Requirement 24.6)

mod config;
mod collector;
mod consent;
mod events;

#[cfg(test)]
mod tests;

pub use config::{TelemetryConfig, TelemetryEndpoint};
pub use collector::{TelemetryCollector, TelemetryBatch};
pub use consent::{ConsentManager, ConsentStatus, ConsentRecord};
pub use events::{TelemetryEvent, EventType, EventData, FeatureUsage, PerformanceEvent, ErrorEvent};

use std::sync::Arc;
use parking_lot::RwLock;
use thiserror::Error;
use chrono::{DateTime, Utc};

/// Telemetry system errors
#[derive(Debug, Error)]
pub enum TelemetryError {
    #[error("Telemetry is disabled by user")]
    Disabled,

    #[error("User consent not given")]
    NoConsent,

    #[error("Failed to send telemetry: {0}")]
    SendError(String),

    #[error("Failed to store consent: {0}")]
    ConsentStorageError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

/// Result type for telemetry operations
pub type TelemetryResult<T> = Result<T, TelemetryError>;

/// Main telemetry system
pub struct TelemetrySystem {
    config: TelemetryConfig,
    consent_manager: Arc<ConsentManager>,
    collector: Arc<RwLock<TelemetryCollector>>,
    installation_id: String,
    session_id: String,
    started_at: DateTime<Utc>,
}

impl TelemetrySystem {
    /// Create a new telemetry system
    pub fn new(config: TelemetryConfig) -> TelemetryResult<Self> {
        let consent_manager = Arc::new(ConsentManager::new(config.consent_file.clone())?);
        let collector = Arc::new(RwLock::new(TelemetryCollector::new(config.clone())));

        // Generate or load installation ID (anonymous, no PII)
        let installation_id = Self::get_or_create_installation_id(&config)?;

        // Generate session ID for this run
        let session_id = uuid::Uuid::new_v4().to_string();

        Ok(Self {
            config,
            consent_manager,
            collector,
            installation_id,
            session_id,
            started_at: Utc::now(),
        })
    }

    /// Get or create an anonymous installation ID
    fn get_or_create_installation_id(config: &TelemetryConfig) -> TelemetryResult<String> {
        let id_file = config.data_directory.join("installation_id");

        if id_file.exists() {
            std::fs::read_to_string(&id_file)
                .map(|s| s.trim().to_string())
                .map_err(|e| TelemetryError::IoError(e))
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            if let Some(parent) = id_file.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&id_file, &id)?;
            Ok(id)
        }
    }

    /// Check if telemetry is enabled and user has consented
    pub fn is_enabled(&self) -> bool {
        self.config.enabled && self.consent_manager.has_consent()
    }

    /// Get the consent manager for UI integration
    pub fn consent_manager(&self) -> Arc<ConsentManager> {
        Arc::clone(&self.consent_manager)
    }

    /// Record a telemetry event
    pub fn record(&self, event: TelemetryEvent) -> TelemetryResult<()> {
        if !self.is_enabled() {
            return Ok(()); // Silently ignore if disabled
        }

        // Ensure no PII is included (Requirement 24.4)
        let sanitized_event = self.sanitize_event(event);

        let mut collector = self.collector.write();
        collector.add_event(sanitized_event);

        Ok(())
    }

    /// Record a feature usage event
    pub fn record_feature_usage(&self, feature: &str, action: &str) -> TelemetryResult<()> {
        let event = TelemetryEvent {
            event_type: EventType::FeatureUsage,
            timestamp: Utc::now(),
            installation_id: self.installation_id.clone(),
            session_id: self.session_id.clone(),
            data: EventData::FeatureUsage(FeatureUsage {
                feature_name: feature.to_string(),
                action: action.to_string(),
                duration_ms: None,
                success: true,
            }),
        };
        self.record(event)
    }

    /// Record a performance event
    pub fn record_performance(&self, operation: &str, duration_ms: u64) -> TelemetryResult<()> {
        let event = TelemetryEvent {
            event_type: EventType::Performance,
            timestamp: Utc::now(),
            installation_id: self.installation_id.clone(),
            session_id: self.session_id.clone(),
            data: EventData::Performance(PerformanceEvent {
                operation: operation.to_string(),
                duration_ms,
                success: true,
                error_code: None,
            }),
        };
        self.record(event)
    }

    /// Record an error event (anonymized)
    pub fn record_error(&self, error_type: &str, error_code: Option<&str>) -> TelemetryResult<()> {
        let event = TelemetryEvent {
            event_type: EventType::Error,
            timestamp: Utc::now(),
            installation_id: self.installation_id.clone(),
            session_id: self.session_id.clone(),
            data: EventData::Error(ErrorEvent {
                error_type: error_type.to_string(),
                error_code: error_code.map(|s| s.to_string()),
                // No stack traces or file paths - Requirement 24.4
                context: None,
            }),
        };
        self.record(event)
    }

    /// Sanitize event to remove any potential PII (Requirement 24.4)
    fn sanitize_event(&self, mut event: TelemetryEvent) -> TelemetryEvent {
        // Remove any potential file paths or usernames from event data
        match &mut event.data {
            EventData::FeatureUsage(usage) => {
                usage.feature_name = Self::sanitize_string(&usage.feature_name);
                usage.action = Self::sanitize_string(&usage.action);
            }
            EventData::Performance(perf) => {
                perf.operation = Self::sanitize_string(&perf.operation);
            }
            EventData::Error(err) => {
                err.error_type = Self::sanitize_string(&err.error_type);
                err.context = err.context.as_ref().map(|c| Self::sanitize_string(c));
            }
            EventData::Session(_) => {
                // Session data is already anonymous
            }
        }
        event
    }

    /// Sanitize a string to remove potential PII
    fn sanitize_string(s: &str) -> String {
        let mut result = s.to_string();

        // Remove potential usernames
        if let Ok(username) = std::env::var("USERNAME") {
            result = result.replace(&username, "[USER]");
        }
        if let Ok(username) = std::env::var("USER") {
            result = result.replace(&username, "[USER]");
        }

        // Remove potential home directory paths
        if let Some(home) = dirs::home_dir() {
            if let Some(home_str) = home.to_str() {
                result = result.replace(home_str, "[HOME]");
            }
        }

        // Remove potential file paths (Windows and Unix)
        let path_pattern = regex::Regex::new(r"[A-Za-z]:\\[^\s]+|/[^\s]+/[^\s]+").unwrap();
        result = path_pattern.replace_all(&result, "[PATH]").to_string();

        result
    }

    /// Flush pending events (send to server if configured)
    pub async fn flush(&self) -> TelemetryResult<()> {
        if !self.is_enabled() {
            return Ok(());
        }

        let mut collector = self.collector.write();
        collector.flush().await
    }

    /// Get current session statistics (for display in settings)
    pub fn get_session_stats(&self) -> SessionStats {
        let collector = self.collector.read();
        SessionStats {
            session_id: self.session_id.clone(),
            started_at: self.started_at,
            events_recorded: collector.event_count(),
            events_sent: collector.sent_count(),
            is_enabled: self.is_enabled(),
            has_consent: self.consent_manager.has_consent(),
        }
    }

    /// Disable telemetry and clear all pending data
    pub fn disable_and_clear(&self) -> TelemetryResult<()> {
        // Revoke consent
        self.consent_manager.revoke_consent()?;

        // Clear pending events
        let mut collector = self.collector.write();
        collector.clear();

        tracing::info!("Telemetry disabled and data cleared");
        Ok(())
    }
}

/// Session statistics for UI display
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionStats {
    pub session_id: String,
    pub started_at: DateTime<Utc>,
    pub events_recorded: usize,
    pub events_sent: usize,
    pub is_enabled: bool,
    pub has_consent: bool,
}

/// Initialize telemetry with default configuration
pub fn init_telemetry() -> TelemetryResult<TelemetrySystem> {
    let config = TelemetryConfig::default();
    TelemetrySystem::new(config)
}

/// Initialize telemetry with custom configuration
pub fn init_telemetry_with_config(config: TelemetryConfig) -> TelemetryResult<TelemetrySystem> {
    TelemetrySystem::new(config)
}
