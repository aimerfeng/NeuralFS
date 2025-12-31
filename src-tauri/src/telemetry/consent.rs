//! User consent management for telemetry
//!
//! Implements explicit user consent requirement (Requirement 24.3)
//! and complete opt-out capability (Requirement 24.6)

use super::TelemetryError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;

/// Consent status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsentStatus {
    /// User has not made a decision yet
    NotAsked,
    /// User has given consent
    Granted,
    /// User has denied consent
    Denied,
    /// User has revoked previously given consent
    Revoked,
}

impl Default for ConsentStatus {
    fn default() -> Self {
        ConsentStatus::NotAsked
    }
}

/// Record of user consent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentRecord {
    /// Current consent status
    pub status: ConsentStatus,
    /// When consent was last updated
    pub updated_at: DateTime<Utc>,
    /// Version of the privacy policy when consent was given
    pub policy_version: String,
    /// Application version when consent was given
    pub app_version: String,
    /// History of consent changes
    pub history: Vec<ConsentHistoryEntry>,
}

/// Entry in consent history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentHistoryEntry {
    pub status: ConsentStatus,
    pub timestamp: DateTime<Utc>,
    pub policy_version: String,
}

impl Default for ConsentRecord {
    fn default() -> Self {
        Self {
            status: ConsentStatus::NotAsked,
            updated_at: Utc::now(),
            policy_version: "1.0".to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            history: Vec::new(),
        }
    }
}

/// Manages user consent for telemetry collection
pub struct ConsentManager {
    consent_file: PathBuf,
    record: RwLock<ConsentRecord>,
    current_policy_version: String,
}

impl ConsentManager {
    /// Create a new consent manager
    pub fn new(consent_file: PathBuf) -> Result<Self, TelemetryError> {
        let record = Self::load_or_create(&consent_file)?;

        Ok(Self {
            consent_file,
            record: RwLock::new(record),
            current_policy_version: "1.0".to_string(),
        })
    }

    /// Load existing consent record or create a new one
    fn load_or_create(consent_file: &PathBuf) -> Result<ConsentRecord, TelemetryError> {
        if consent_file.exists() {
            let content = std::fs::read_to_string(consent_file)?;
            serde_json::from_str(&content)
                .map_err(|e| TelemetryError::SerializationError(e.to_string()))
        } else {
            Ok(ConsentRecord::default())
        }
    }

    /// Save consent record to file
    fn save(&self) -> Result<(), TelemetryError> {
        let record = self.record.read();

        // Ensure parent directory exists
        if let Some(parent) = self.consent_file.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(&*record)
            .map_err(|e| TelemetryError::SerializationError(e.to_string()))?;

        std::fs::write(&self.consent_file, content)?;
        Ok(())
    }

    /// Check if user has given consent
    pub fn has_consent(&self) -> bool {
        let record = self.record.read();
        record.status == ConsentStatus::Granted
    }

    /// Get current consent status
    pub fn status(&self) -> ConsentStatus {
        let record = self.record.read();
        record.status
    }

    /// Check if consent needs to be re-requested (e.g., policy update)
    pub fn needs_consent_update(&self) -> bool {
        let record = self.record.read();
        match record.status {
            ConsentStatus::NotAsked => true,
            ConsentStatus::Granted => record.policy_version != self.current_policy_version,
            ConsentStatus::Denied | ConsentStatus::Revoked => false,
        }
    }

    /// Grant consent (Requirement 24.3 - explicit consent)
    pub fn grant_consent(&self) -> Result<(), TelemetryError> {
        let mut record = self.record.write();

        // Add to history
        record.history.push(ConsentHistoryEntry {
            status: record.status,
            timestamp: record.updated_at,
            policy_version: record.policy_version.clone(),
        });

        // Update record
        record.status = ConsentStatus::Granted;
        record.updated_at = Utc::now();
        record.policy_version = self.current_policy_version.clone();
        record.app_version = env!("CARGO_PKG_VERSION").to_string();

        drop(record);
        self.save()?;

        tracing::info!("Telemetry consent granted");
        Ok(())
    }

    /// Deny consent
    pub fn deny_consent(&self) -> Result<(), TelemetryError> {
        let mut record = self.record.write();

        // Add to history
        record.history.push(ConsentHistoryEntry {
            status: record.status,
            timestamp: record.updated_at,
            policy_version: record.policy_version.clone(),
        });

        // Update record
        record.status = ConsentStatus::Denied;
        record.updated_at = Utc::now();
        record.policy_version = self.current_policy_version.clone();
        record.app_version = env!("CARGO_PKG_VERSION").to_string();

        drop(record);
        self.save()?;

        tracing::info!("Telemetry consent denied");
        Ok(())
    }

    /// Revoke previously given consent (Requirement 24.6 - opt-out at any time)
    pub fn revoke_consent(&self) -> Result<(), TelemetryError> {
        let mut record = self.record.write();

        // Add to history
        record.history.push(ConsentHistoryEntry {
            status: record.status,
            timestamp: record.updated_at,
            policy_version: record.policy_version.clone(),
        });

        // Update record
        record.status = ConsentStatus::Revoked;
        record.updated_at = Utc::now();

        drop(record);
        self.save()?;

        tracing::info!("Telemetry consent revoked");
        Ok(())
    }

    /// Get the full consent record
    pub fn get_record(&self) -> ConsentRecord {
        let record = self.record.read();
        record.clone()
    }

    /// Get consent history
    pub fn get_history(&self) -> Vec<ConsentHistoryEntry> {
        let record = self.record.read();
        record.history.clone()
    }

    /// Get current policy version
    pub fn current_policy_version(&self) -> &str {
        &self.current_policy_version
    }

    /// Delete all consent data (for GDPR compliance)
    pub fn delete_all_data(&self) -> Result<(), TelemetryError> {
        // Reset to default
        {
            let mut record = self.record.write();
            *record = ConsentRecord::default();
        }

        // Delete the file if it exists
        if self.consent_file.exists() {
            std::fs::remove_file(&self.consent_file)?;
        }

        tracing::info!("All consent data deleted");
        Ok(())
    }
}

/// Information to display in consent dialog
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentDialogInfo {
    /// Title of the dialog
    pub title: String,
    /// Main message explaining what data is collected
    pub message: String,
    /// List of data types that will be collected
    pub data_collected: Vec<String>,
    /// List of data types that will NOT be collected
    pub data_not_collected: Vec<String>,
    /// Link to full privacy policy
    pub privacy_policy_url: Option<String>,
    /// Current policy version
    pub policy_version: String,
}

impl Default for ConsentDialogInfo {
    fn default() -> Self {
        Self {
            title: "Help Improve NeuralFS".to_string(),
            message: "We collect anonymous usage statistics to improve NeuralFS. \
                      No personal information or file contents are ever collected.".to_string(),
            data_collected: vec![
                "Feature usage (which features you use)".to_string(),
                "Performance metrics (how fast operations complete)".to_string(),
                "Error types (what kinds of errors occur)".to_string(),
                "Application version and platform".to_string(),
            ],
            data_not_collected: vec![
                "File names or contents".to_string(),
                "File paths or directory structures".to_string(),
                "Personal information (name, email, etc.)".to_string(),
                "Search queries or tags".to_string(),
                "Any data that could identify you".to_string(),
            ],
            privacy_policy_url: None,
            policy_version: "1.0".to_string(),
        }
    }
}

/// Get consent dialog information for UI
pub fn get_consent_dialog_info() -> ConsentDialogInfo {
    ConsentDialogInfo::default()
}
