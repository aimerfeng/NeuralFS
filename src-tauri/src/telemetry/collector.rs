//! Telemetry event collection and batching

use super::{TelemetryConfig, TelemetryError, TelemetryEvent};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use chrono::{DateTime, Utc};

/// A batch of telemetry events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryBatch {
    /// Batch ID
    pub batch_id: String,
    /// Events in this batch
    pub events: Vec<TelemetryEvent>,
    /// When the batch was created
    pub created_at: DateTime<Utc>,
    /// Application version
    pub app_version: String,
    /// Platform
    pub platform: String,
}

impl TelemetryBatch {
    /// Create a new batch from events
    pub fn new(events: Vec<TelemetryEvent>, app_version: String, platform: String) -> Self {
        Self {
            batch_id: uuid::Uuid::new_v4().to_string(),
            events,
            created_at: Utc::now(),
            app_version,
            platform,
        }
    }

    /// Check if the batch is empty
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Get the number of events in the batch
    pub fn len(&self) -> usize {
        self.events.len()
    }
}

/// Collects and batches telemetry events
pub struct TelemetryCollector {
    config: TelemetryConfig,
    events: VecDeque<TelemetryEvent>,
    pending_batches: VecDeque<TelemetryBatch>,
    event_count: usize,
    sent_count: usize,
    last_flush: Option<DateTime<Utc>>,
}

impl TelemetryCollector {
    /// Create a new telemetry collector
    pub fn new(config: TelemetryConfig) -> Self {
        Self {
            config,
            events: VecDeque::new(),
            pending_batches: VecDeque::new(),
            event_count: 0,
            sent_count: 0,
            last_flush: None,
        }
    }

    /// Add an event to the collector
    pub fn add_event(&mut self, event: TelemetryEvent) {
        // Check if we should sample this event (for performance events)
        if !self.should_sample(&event) {
            return;
        }

        // Add event
        self.events.push_back(event);
        self.event_count += 1;

        // Trim if we exceed max local events
        while self.events.len() > self.config.max_local_events {
            self.events.pop_front();
        }

        // Check if we should create a batch
        if self.events.len() >= self.config.batch_size {
            self.create_batch();
        }
    }

    /// Check if an event should be sampled
    fn should_sample(&self, event: &TelemetryEvent) -> bool {
        use super::events::EventType;

        match event.event_type {
            EventType::Performance => {
                // Apply sampling rate to performance events
                rand::random::<f64>() < self.config.performance_sampling_rate
            }
            _ => true, // Always collect other event types
        }
    }

    /// Create a batch from current events
    fn create_batch(&mut self) {
        if self.events.is_empty() {
            return;
        }

        let batch_events: Vec<TelemetryEvent> = self
            .events
            .drain(..self.config.batch_size.min(self.events.len()))
            .collect();

        let batch = TelemetryBatch::new(
            batch_events,
            self.config.app_version.clone(),
            self.config.platform.clone(),
        );

        self.pending_batches.push_back(batch);

        // Limit pending batches
        while self.pending_batches.len() > 10 {
            self.pending_batches.pop_front();
        }
    }

    /// Flush pending events (send to server)
    pub async fn flush(&mut self) -> Result<(), TelemetryError> {
        // Create batch from remaining events
        if !self.events.is_empty() {
            self.create_batch();
        }

        // Send pending batches
        while let Some(batch) = self.pending_batches.pop_front() {
            match self.send_batch(&batch).await {
                Ok(_) => {
                    self.sent_count += batch.len();
                    tracing::debug!("Sent telemetry batch with {} events", batch.len());
                }
                Err(e) => {
                    // Put batch back for retry
                    self.pending_batches.push_front(batch);
                    tracing::warn!("Failed to send telemetry batch: {}", e);
                    return Err(e);
                }
            }
        }

        self.last_flush = Some(Utc::now());
        Ok(())
    }

    /// Send a batch to the telemetry server
    async fn send_batch(&self, batch: &TelemetryBatch) -> Result<(), TelemetryError> {
        if !self.config.has_endpoint() {
            // No endpoint configured, just log locally
            tracing::trace!("Telemetry batch (no endpoint): {:?}", batch.batch_id);
            return Ok(());
        }

        let client = reqwest::Client::new();
        let mut request = client
            .post(&self.config.endpoint.url)
            .timeout(self.config.endpoint_timeout())
            .json(batch);

        // Add API key if configured
        if let Some(ref api_key) = self.config.endpoint.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = request
            .send()
            .await
            .map_err(|e| TelemetryError::SendError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(TelemetryError::SendError(format!(
                "Server returned status: {}",
                response.status()
            )));
        }

        Ok(())
    }

    /// Get the number of events recorded
    pub fn event_count(&self) -> usize {
        self.event_count
    }

    /// Get the number of events sent
    pub fn sent_count(&self) -> usize {
        self.sent_count
    }

    /// Get the number of pending events
    pub fn pending_count(&self) -> usize {
        self.events.len() + self.pending_batches.iter().map(|b| b.len()).sum::<usize>()
    }

    /// Get the last flush time
    pub fn last_flush(&self) -> Option<DateTime<Utc>> {
        self.last_flush
    }

    /// Clear all pending events and batches
    pub fn clear(&mut self) {
        self.events.clear();
        self.pending_batches.clear();
        self.event_count = 0;
        self.sent_count = 0;
        self.last_flush = None;
    }

    /// Export pending events as JSON (for debugging)
    pub fn export_pending_json(&self) -> String {
        let all_events: Vec<&TelemetryEvent> = self
            .events
            .iter()
            .chain(self.pending_batches.iter().flat_map(|b| b.events.iter()))
            .collect();

        serde_json::to_string_pretty(&all_events).unwrap_or_else(|_| "[]".to_string())
    }
}
