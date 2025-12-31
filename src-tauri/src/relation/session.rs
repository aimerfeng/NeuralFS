//! Session Tracker - Track files opened in the same session
//!
//! Records which files are opened together to build session-based relations.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};
use uuid::Uuid;

use crate::core::types::{FileRelation, RelationType, RelationSource, UserFeedback};
use super::error::{RelationError, Result};

/// Configuration for the SessionTracker
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Maximum session duration before auto-ending (in minutes)
    pub max_session_duration_minutes: u64,
    /// Minimum files in session to create relations
    pub min_files_for_relations: usize,
    /// Session idle timeout (in minutes)
    pub idle_timeout_minutes: u64,
    /// Base strength for session-based relations
    pub base_relation_strength: f32,
    /// Strength boost per additional co-occurrence
    pub co_occurrence_boost: f32,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            max_session_duration_minutes: 480, // 8 hours
            min_files_for_relations: 2,
            idle_timeout_minutes: 30,
            base_relation_strength: 0.3,
            co_occurrence_boost: 0.1,
        }
    }
}

/// Information about a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// Session ID
    pub id: Uuid,
    /// Session start time
    pub started_at: DateTime<Utc>,
    /// Session end time (None if still active)
    pub ended_at: Option<DateTime<Utc>>,
    /// Last activity time
    pub last_activity_at: DateTime<Utc>,
    /// Files opened in this session
    pub file_ids: Vec<Uuid>,
    /// Whether the session is active
    pub is_active: bool,
}

/// Event recorded in a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvent {
    /// Event ID
    pub id: Uuid,
    /// Session ID
    pub session_id: Uuid,
    /// File ID
    pub file_id: Uuid,
    /// Event type
    pub event_type: SessionEventType,
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
}

/// Type of session event
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionEventType {
    /// File was opened
    FileOpened,
    /// File was closed
    FileClosed,
    /// File was modified
    FileModified,
    /// File was accessed (read)
    FileAccessed,
}

/// Session Tracker - manages session-based file associations
pub struct SessionTracker {
    pool: SqlitePool,
    config: SessionConfig,
    /// Current active session
    current_session: Arc<RwLock<Option<SessionInfo>>>,
    /// In-memory cache of session files for quick lookup
    session_files: Arc<RwLock<HashSet<Uuid>>>,
}

impl SessionTracker {
    /// Create a new SessionTracker
    pub async fn new(pool: SqlitePool, config: SessionConfig) -> Result<Self> {
        let tracker = Self {
            pool,
            config,
            current_session: Arc::new(RwLock::new(None)),
            session_files: Arc::new(RwLock::new(HashSet::new())),
        };

        // Check for any active sessions that need to be ended
        tracker.cleanup_stale_sessions().await?;

        Ok(tracker)
    }

    /// Start a new session
    pub async fn start_session(&self) -> Result<SessionInfo> {
        // End any existing session first
        if let Some(session) = self.current_session.read().await.as_ref() {
            if session.is_active {
                self.end_session(session.id).await?;
            }
        }

        let now = Utc::now();
        let session = SessionInfo {
            id: Uuid::now_v7(),
            started_at: now,
            ended_at: None,
            last_activity_at: now,
            file_ids: Vec::new(),
            is_active: true,
        };

        // Insert into database
        sqlx::query(
            r#"
            INSERT INTO sessions (id, started_at, ended_at, last_activity_at, is_active)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(session.id.to_string())
        .bind(session.started_at.to_rfc3339())
        .bind(session.ended_at.map(|t| t.to_rfc3339()))
        .bind(session.last_activity_at.to_rfc3339())
        .bind(session.is_active)
        .execute(&self.pool)
        .await?;

        // Update in-memory state
        *self.current_session.write().await = Some(session.clone());
        self.session_files.write().await.clear();

        info!("Started new session: {}", session.id);
        Ok(session)
    }

    /// End a session
    pub async fn end_session(&self, session_id: Uuid) -> Result<SessionInfo> {
        let now = Utc::now();

        // Update database
        let result = sqlx::query(
            r#"
            UPDATE sessions SET ended_at = ?, is_active = 0
            WHERE id = ? AND is_active = 1
            "#,
        )
        .bind(now.to_rfc3339())
        .bind(session_id.to_string())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            // Check if session exists but is already ended
            let exists = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM sessions WHERE id = ?"
            )
            .bind(session_id.to_string())
            .fetch_one(&self.pool)
            .await?;

            if exists > 0 {
                return Err(RelationError::SessionAlreadyEnded { id: session_id });
            } else {
                return Err(RelationError::SessionNotFound { id: session_id });
            }
        }

        // Get the updated session
        let session = self.get_session(session_id).await?
            .ok_or(RelationError::SessionNotFound { id: session_id })?;

        // Clear in-memory state if this was the current session
        let mut current = self.current_session.write().await;
        if current.as_ref().map(|s| s.id) == Some(session_id) {
            *current = None;
            self.session_files.write().await.clear();
        }

        info!("Ended session: {}", session_id);
        Ok(session)
    }

    /// Get a session by ID
    pub async fn get_session(&self, session_id: Uuid) -> Result<Option<SessionInfo>> {
        let row = sqlx::query_as::<_, SessionRow>(
            r#"
            SELECT id, started_at, ended_at, last_activity_at, is_active
            FROM sessions WHERE id = ?
            "#,
        )
        .bind(session_id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(r) => {
                let mut session = r.into_session()?;
                // Load file IDs
                session.file_ids = self.get_session_file_ids(session_id).await?;
                Ok(Some(session))
            }
            None => Ok(None),
        }
    }

    /// Get the current active session, creating one if needed
    pub async fn get_or_create_session(&self) -> Result<SessionInfo> {
        // Check if we have an active session
        if let Some(session) = self.current_session.read().await.as_ref() {
            if session.is_active {
                // Check if session has timed out
                let idle_duration = Utc::now() - session.last_activity_at;
                if idle_duration > Duration::minutes(self.config.idle_timeout_minutes as i64) {
                    // Session timed out, end it and start a new one
                    drop(self.current_session.read().await);
                    self.end_session(session.id).await?;
                    return self.start_session().await;
                }
                return Ok(session.clone());
            }
        }

        // No active session, start a new one
        self.start_session().await
    }

    /// Record a file event in the current session
    pub async fn record_file_event(
        &self,
        file_id: Uuid,
        event_type: SessionEventType,
    ) -> Result<SessionEvent> {
        let session = self.get_or_create_session().await?;
        let now = Utc::now();

        let event = SessionEvent {
            id: Uuid::now_v7(),
            session_id: session.id,
            file_id,
            event_type,
            timestamp: now,
        };

        // Insert event into database
        let event_type_str = format!("{:?}", event_type);
        sqlx::query(
            r#"
            INSERT INTO session_events (id, session_id, file_id, event_type, timestamp)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(event.id.to_string())
        .bind(event.session_id.to_string())
        .bind(event.file_id.to_string())
        .bind(&event_type_str)
        .bind(event.timestamp.to_rfc3339())
        .execute(&self.pool)
        .await?;

        // Update session last activity
        sqlx::query("UPDATE sessions SET last_activity_at = ? WHERE id = ?")
            .bind(now.to_rfc3339())
            .bind(session.id.to_string())
            .execute(&self.pool)
            .await?;

        // Update in-memory cache
        self.session_files.write().await.insert(file_id);
        if let Some(ref mut s) = *self.current_session.write().await {
            s.last_activity_at = now;
            if !s.file_ids.contains(&file_id) {
                s.file_ids.push(file_id);
            }
        }

        debug!("Recorded {:?} event for file {} in session {}", event_type, file_id, session.id);
        Ok(event)
    }

    /// Get all files in a session
    pub async fn get_session_file_ids(&self, session_id: Uuid) -> Result<Vec<Uuid>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            r#"
            SELECT DISTINCT file_id FROM session_events 
            WHERE session_id = ?
            ORDER BY timestamp ASC
            "#,
        )
        .bind(session_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|(id,)| Uuid::parse_str(&id).map_err(|e| RelationError::Internal(e.to_string())))
            .collect()
    }

    /// Generate session-based relations for a completed session
    pub async fn generate_session_relations(&self, session_id: Uuid) -> Result<Vec<FileRelation>> {
        let file_ids = self.get_session_file_ids(session_id).await?;
        
        if file_ids.len() < self.config.min_files_for_relations {
            debug!(
                "Session {} has only {} files, skipping relation generation",
                session_id,
                file_ids.len()
            );
            return Ok(vec![]);
        }

        let mut relations = Vec::new();
        let now = Utc::now();

        // Create relations between all pairs of files in the session
        for i in 0..file_ids.len() {
            for j in (i + 1)..file_ids.len() {
                let source_id = file_ids[i];
                let target_id = file_ids[j];

                // Calculate strength based on co-occurrence count
                let co_occurrence_count = self.count_co_occurrences(source_id, target_id).await?;
                let strength = (self.config.base_relation_strength
                    + (co_occurrence_count as f32 - 1.0) * self.config.co_occurrence_boost)
                    .min(1.0);

                let relation = FileRelation {
                    id: Uuid::now_v7(),
                    source_file_id: source_id,
                    target_file_id: target_id,
                    relation_type: RelationType::SameSession,
                    strength,
                    source: RelationSource::SessionTracking,
                    user_feedback: UserFeedback::None,
                    created_at: now,
                    updated_at: now,
                    user_action_at: None,
                };

                relations.push(relation);
            }
        }

        info!(
            "Generated {} session relations for session {}",
            relations.len(),
            session_id
        );
        Ok(relations)
    }

    /// Count how many sessions two files have appeared together in
    async fn count_co_occurrences(&self, file_a: Uuid, file_b: Uuid) -> Result<u32> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(DISTINCT e1.session_id)
            FROM session_events e1
            JOIN session_events e2 ON e1.session_id = e2.session_id
            WHERE e1.file_id = ? AND e2.file_id = ?
            "#,
        )
        .bind(file_a.to_string())
        .bind(file_b.to_string())
        .fetch_one(&self.pool)
        .await?;

        Ok(count as u32)
    }

    /// Clean up stale sessions (sessions that were never properly ended)
    async fn cleanup_stale_sessions(&self) -> Result<u64> {
        let max_duration = Duration::minutes(self.config.max_session_duration_minutes as i64);
        let cutoff = Utc::now() - max_duration;

        let result = sqlx::query(
            r#"
            UPDATE sessions SET ended_at = last_activity_at, is_active = 0
            WHERE is_active = 1 AND last_activity_at < ?
            "#,
        )
        .bind(cutoff.to_rfc3339())
        .execute(&self.pool)
        .await?;

        let cleaned = result.rows_affected();
        if cleaned > 0 {
            info!("Cleaned up {} stale sessions", cleaned);
        }

        Ok(cleaned)
    }

    /// Get files that were opened in the same session as the given file
    pub async fn get_session_related_files(&self, file_id: Uuid) -> Result<Vec<Uuid>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            r#"
            SELECT DISTINCT e2.file_id
            FROM session_events e1
            JOIN session_events e2 ON e1.session_id = e2.session_id
            WHERE e1.file_id = ? AND e2.file_id != ?
            "#,
        )
        .bind(file_id.to_string())
        .bind(file_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|(id,)| Uuid::parse_str(&id).map_err(|e| RelationError::Internal(e.to_string())))
            .collect()
    }
}

// ============================================================================
// Database Row Types
// ============================================================================

#[derive(sqlx::FromRow)]
struct SessionRow {
    id: String,
    started_at: String,
    ended_at: Option<String>,
    last_activity_at: String,
    is_active: bool,
}

impl SessionRow {
    fn into_session(self) -> Result<SessionInfo> {
        Ok(SessionInfo {
            id: Uuid::parse_str(&self.id).map_err(|e| RelationError::Internal(e.to_string()))?,
            started_at: DateTime::parse_from_rfc3339(&self.started_at)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| RelationError::Internal(e.to_string()))?,
            ended_at: self.ended_at
                .as_deref()
                .map(|s| DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&Utc)))
                .transpose()
                .map_err(|e| RelationError::Internal(e.to_string()))?,
            last_activity_at: DateTime::parse_from_rfc3339(&self.last_activity_at)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| RelationError::Internal(e.to_string()))?,
            file_ids: Vec::new(), // Loaded separately
            is_active: self.is_active,
        })
    }
}
