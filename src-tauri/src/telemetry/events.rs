//! Telemetry event types
//!
//! Defines the types of events that can be collected.
//! All events are designed to be anonymous and contain no PII (Requirement 24.4)

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Type of telemetry event
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    /// Feature usage event
    FeatureUsage,
    /// Performance measurement event
    Performance,
    /// Error event (anonymized)
    Error,
    /// Session event (start/end)
    Session,
}

/// Telemetry event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    /// Type of event
    pub event_type: EventType,
    /// When the event occurred
    pub timestamp: DateTime<Utc>,
    /// Anonymous installation ID
    pub installation_id: String,
    /// Session ID
    pub session_id: String,
    /// Event-specific data
    pub data: EventData,
}

/// Event-specific data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventData {
    /// Feature usage data
    FeatureUsage(FeatureUsage),
    /// Performance data
    Performance(PerformanceEvent),
    /// Error data
    Error(ErrorEvent),
    /// Session data
    Session(SessionEvent),
}

/// Feature usage event data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureUsage {
    /// Name of the feature (e.g., "search", "tag_management", "relation_view")
    pub feature_name: String,
    /// Action performed (e.g., "open", "close", "use")
    pub action: String,
    /// Duration of usage in milliseconds (if applicable)
    pub duration_ms: Option<u64>,
    /// Whether the action was successful
    pub success: bool,
}

/// Performance event data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceEvent {
    /// Name of the operation (e.g., "search_query", "file_index", "embedding_generate")
    pub operation: String,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Whether the operation succeeded
    pub success: bool,
    /// Error code if failed (no details, just code)
    pub error_code: Option<String>,
}

/// Error event data (anonymized - no stack traces or file paths)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEvent {
    /// Type of error (e.g., "database_error", "network_error", "parse_error")
    pub error_type: String,
    /// Error code (if applicable)
    pub error_code: Option<String>,
    /// Anonymized context (no file paths or PII)
    pub context: Option<String>,
}

/// Session event data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvent {
    /// Session action
    pub action: SessionAction,
    /// Session duration in seconds (for end events)
    pub duration_secs: Option<u64>,
    /// Platform information
    pub platform: PlatformInfo,
}

/// Session action type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionAction {
    Start,
    End,
    Resume,
    Suspend,
}

/// Platform information (anonymous)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformInfo {
    /// Operating system (e.g., "windows", "macos", "linux")
    pub os: String,
    /// OS version (major.minor only)
    pub os_version: Option<String>,
    /// Architecture (e.g., "x86_64", "aarch64")
    pub arch: String,
    /// Application version
    pub app_version: String,
    /// Whether GPU is available
    pub has_gpu: bool,
}

impl Default for PlatformInfo {
    fn default() -> Self {
        Self {
            os: std::env::consts::OS.to_string(),
            os_version: None, // Could be populated from system info
            arch: std::env::consts::ARCH.to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            has_gpu: false, // Should be set based on actual detection
        }
    }
}

impl TelemetryEvent {
    /// Create a new feature usage event
    pub fn feature_usage(
        installation_id: String,
        session_id: String,
        feature: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self {
            event_type: EventType::FeatureUsage,
            timestamp: Utc::now(),
            installation_id,
            session_id,
            data: EventData::FeatureUsage(FeatureUsage {
                feature_name: feature.into(),
                action: action.into(),
                duration_ms: None,
                success: true,
            }),
        }
    }

    /// Create a new performance event
    pub fn performance(
        installation_id: String,
        session_id: String,
        operation: impl Into<String>,
        duration_ms: u64,
        success: bool,
    ) -> Self {
        Self {
            event_type: EventType::Performance,
            timestamp: Utc::now(),
            installation_id,
            session_id,
            data: EventData::Performance(PerformanceEvent {
                operation: operation.into(),
                duration_ms,
                success,
                error_code: None,
            }),
        }
    }

    /// Create a new error event
    pub fn error(
        installation_id: String,
        session_id: String,
        error_type: impl Into<String>,
        error_code: Option<String>,
    ) -> Self {
        Self {
            event_type: EventType::Error,
            timestamp: Utc::now(),
            installation_id,
            session_id,
            data: EventData::Error(ErrorEvent {
                error_type: error_type.into(),
                error_code,
                context: None,
            }),
        }
    }

    /// Create a session start event
    pub fn session_start(installation_id: String, session_id: String, has_gpu: bool) -> Self {
        Self {
            event_type: EventType::Session,
            timestamp: Utc::now(),
            installation_id,
            session_id,
            data: EventData::Session(SessionEvent {
                action: SessionAction::Start,
                duration_secs: None,
                platform: PlatformInfo {
                    has_gpu,
                    ..Default::default()
                },
            }),
        }
    }

    /// Create a session end event
    pub fn session_end(
        installation_id: String,
        session_id: String,
        duration_secs: u64,
    ) -> Self {
        Self {
            event_type: EventType::Session,
            timestamp: Utc::now(),
            installation_id,
            session_id,
            data: EventData::Session(SessionEvent {
                action: SessionAction::End,
                duration_secs: Some(duration_secs),
                platform: PlatformInfo::default(),
            }),
        }
    }
}

/// Predefined feature names for consistency
pub mod features {
    pub const SEARCH: &str = "search";
    pub const TAG_MANAGEMENT: &str = "tag_management";
    pub const RELATION_VIEW: &str = "relation_view";
    pub const FILE_PREVIEW: &str = "file_preview";
    pub const SETTINGS: &str = "settings";
    pub const ONBOARDING: &str = "onboarding";
    pub const CLOUD_INFERENCE: &str = "cloud_inference";
    pub const LOCAL_INFERENCE: &str = "local_inference";
}

/// Predefined operation names for consistency
pub mod operations {
    pub const SEARCH_QUERY: &str = "search_query";
    pub const FILE_INDEX: &str = "file_index";
    pub const EMBEDDING_GENERATE: &str = "embedding_generate";
    pub const TAG_SUGGEST: &str = "tag_suggest";
    pub const RELATION_COMPUTE: &str = "relation_compute";
    pub const CLOUD_API_CALL: &str = "cloud_api_call";
    pub const DATABASE_QUERY: &str = "database_query";
    pub const FILE_PARSE: &str = "file_parse";
}

/// Predefined error types for consistency
pub mod error_types {
    pub const DATABASE_ERROR: &str = "database_error";
    pub const NETWORK_ERROR: &str = "network_error";
    pub const PARSE_ERROR: &str = "parse_error";
    pub const EMBEDDING_ERROR: &str = "embedding_error";
    pub const INFERENCE_ERROR: &str = "inference_error";
    pub const FILE_SYSTEM_ERROR: &str = "file_system_error";
    pub const CONFIGURATION_ERROR: &str = "configuration_error";
}
