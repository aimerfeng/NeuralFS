//! Performance metrics collection
//!
//! Implements performance metrics for bottleneck identification (Requirement 24.7)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use parking_lot::RwLock;
use chrono::{DateTime, Utc};

/// Type of metric being recorded
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricType {
    /// Duration of an operation
    Duration(Duration),
    /// Counter value
    Counter(u64),
    /// Gauge value (can go up or down)
    Gauge(f64),
    /// Histogram bucket
    Histogram(f64),
}

/// A single metric entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricEntry {
    /// Name of the metric
    pub name: String,
    /// Metric value
    pub value: MetricType,
    /// Timestamp when recorded
    pub timestamp: DateTime<Utc>,
    /// Optional labels/tags
    pub labels: HashMap<String, String>,
}

/// Aggregated statistics for a metric
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetricStats {
    /// Number of samples
    pub count: u64,
    /// Sum of all values
    pub sum: f64,
    /// Minimum value
    pub min: f64,
    /// Maximum value
    pub max: f64,
    /// Mean value
    pub mean: f64,
    /// Last recorded value
    pub last: f64,
    /// Last update timestamp
    pub last_updated: Option<DateTime<Utc>>,
}

impl MetricStats {
    fn new() -> Self {
        Self {
            count: 0,
            sum: 0.0,
            min: f64::MAX,
            max: f64::MIN,
            mean: 0.0,
            last: 0.0,
            last_updated: None,
        }
    }

    fn update(&mut self, value: f64) {
        self.count += 1;
        self.sum += value;
        self.min = self.min.min(value);
        self.max = self.max.max(value);
        self.mean = self.sum / self.count as f64;
        self.last = value;
        self.last_updated = Some(Utc::now());
    }
}

/// Performance metrics for the application
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PerformanceMetrics {
    /// Search operation metrics
    pub search_latency_ms: MetricStats,
    /// Indexing operation metrics
    pub indexing_duration_ms: MetricStats,
    /// Embedding generation metrics
    pub embedding_duration_ms: MetricStats,
    /// Cloud API call metrics
    pub cloud_api_latency_ms: MetricStats,
    /// Database query metrics
    pub db_query_duration_ms: MetricStats,
    /// File parsing metrics
    pub file_parse_duration_ms: MetricStats,
    /// Memory usage (bytes)
    pub memory_usage_bytes: MetricStats,
    /// VRAM usage (bytes)
    pub vram_usage_bytes: MetricStats,
    /// Active tasks count
    pub active_tasks: MetricStats,
    /// Queue depth
    pub queue_depth: MetricStats,
}

/// Metrics collector for recording and aggregating performance data
pub struct MetricsCollector {
    /// Aggregated metrics by name
    metrics: RwLock<HashMap<String, MetricStats>>,
    /// Recent entries for detailed analysis
    recent_entries: RwLock<Vec<MetricEntry>>,
    /// Maximum number of recent entries to keep
    max_recent_entries: usize,
    /// Total metrics recorded
    total_recorded: AtomicU64,
    /// Start time for uptime calculation
    start_time: Instant,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self {
            metrics: RwLock::new(HashMap::new()),
            recent_entries: RwLock::new(Vec::new()),
            max_recent_entries: 1000,
            total_recorded: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }

    /// Create a new metrics collector with custom capacity
    pub fn with_capacity(max_recent_entries: usize) -> Self {
        Self {
            metrics: RwLock::new(HashMap::new()),
            recent_entries: RwLock::new(Vec::with_capacity(max_recent_entries)),
            max_recent_entries,
            total_recorded: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }

    /// Record a metric
    pub fn record(&self, name: &str, value: MetricType) {
        self.record_with_labels(name, value, HashMap::new());
    }

    /// Record a metric with labels
    pub fn record_with_labels(
        &self,
        name: &str,
        value: MetricType,
        labels: HashMap<String, String>,
    ) {
        let numeric_value = match &value {
            MetricType::Duration(d) => d.as_millis() as f64,
            MetricType::Counter(c) => *c as f64,
            MetricType::Gauge(g) => *g,
            MetricType::Histogram(h) => *h,
        };

        // Update aggregated stats
        {
            let mut metrics = self.metrics.write();
            let stats = metrics.entry(name.to_string()).or_insert_with(MetricStats::new);
            stats.update(numeric_value);
        }

        // Store recent entry
        {
            let mut recent = self.recent_entries.write();
            if recent.len() >= self.max_recent_entries {
                recent.remove(0);
            }
            recent.push(MetricEntry {
                name: name.to_string(),
                value,
                timestamp: Utc::now(),
                labels,
            });
        }

        self.total_recorded.fetch_add(1, Ordering::Relaxed);

        // Log performance metrics at trace level
        tracing::trace!(
            target: "metrics",
            metric_name = name,
            metric_value = numeric_value,
            "Metric recorded"
        );
    }

    /// Record a duration metric
    pub fn record_duration(&self, name: &str, duration: Duration) {
        self.record(name, MetricType::Duration(duration));
    }

    /// Record a counter metric
    pub fn record_counter(&self, name: &str, value: u64) {
        self.record(name, MetricType::Counter(value));
    }

    /// Record a gauge metric
    pub fn record_gauge(&self, name: &str, value: f64) {
        self.record(name, MetricType::Gauge(value));
    }

    /// Increment a counter
    pub fn increment(&self, name: &str) {
        let mut metrics = self.metrics.write();
        let stats = metrics.entry(name.to_string()).or_insert_with(MetricStats::new);
        stats.update(stats.last + 1.0);
    }

    /// Get statistics for a specific metric
    pub fn get_stats(&self, name: &str) -> Option<MetricStats> {
        let metrics = self.metrics.read();
        metrics.get(name).cloned()
    }

    /// Get all metric statistics
    pub fn get_all_stats(&self) -> HashMap<String, MetricStats> {
        let metrics = self.metrics.read();
        metrics.clone()
    }

    /// Get recent entries
    pub fn get_recent_entries(&self) -> Vec<MetricEntry> {
        let recent = self.recent_entries.read();
        recent.clone()
    }

    /// Get recent entries for a specific metric
    pub fn get_recent_entries_for(&self, name: &str) -> Vec<MetricEntry> {
        let recent = self.recent_entries.read();
        recent
            .iter()
            .filter(|e| e.name == name)
            .cloned()
            .collect()
    }

    /// Get total number of metrics recorded
    pub fn total_recorded(&self) -> u64 {
        self.total_recorded.load(Ordering::Relaxed)
    }

    /// Get uptime duration
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Clear all metrics
    pub fn clear(&self) {
        let mut metrics = self.metrics.write();
        metrics.clear();
        let mut recent = self.recent_entries.write();
        recent.clear();
        self.total_recorded.store(0, Ordering::Relaxed);
    }

    /// Get a summary of performance metrics
    pub fn get_performance_summary(&self) -> PerformanceMetrics {
        let metrics = self.metrics.read();

        PerformanceMetrics {
            search_latency_ms: metrics.get("search_latency").cloned().unwrap_or_default(),
            indexing_duration_ms: metrics.get("indexing_duration").cloned().unwrap_or_default(),
            embedding_duration_ms: metrics.get("embedding_duration").cloned().unwrap_or_default(),
            cloud_api_latency_ms: metrics.get("cloud_api_latency").cloned().unwrap_or_default(),
            db_query_duration_ms: metrics.get("db_query_duration").cloned().unwrap_or_default(),
            file_parse_duration_ms: metrics.get("file_parse_duration").cloned().unwrap_or_default(),
            memory_usage_bytes: metrics.get("memory_usage").cloned().unwrap_or_default(),
            vram_usage_bytes: metrics.get("vram_usage").cloned().unwrap_or_default(),
            active_tasks: metrics.get("active_tasks").cloned().unwrap_or_default(),
            queue_depth: metrics.get("queue_depth").cloned().unwrap_or_default(),
        }
    }

    /// Export metrics as JSON
    pub fn export_json(&self) -> String {
        let stats = self.get_all_stats();
        serde_json::to_string_pretty(&stats).unwrap_or_else(|_| "{}".to_string())
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Timer guard for automatic duration recording
pub struct TimerGuard<'a> {
    collector: &'a MetricsCollector,
    name: String,
    start: Instant,
    labels: HashMap<String, String>,
}

impl<'a> TimerGuard<'a> {
    /// Create a new timer guard
    pub fn new(collector: &'a MetricsCollector, name: impl Into<String>) -> Self {
        Self {
            collector,
            name: name.into(),
            start: Instant::now(),
            labels: HashMap::new(),
        }
    }

    /// Create a new timer guard with labels
    pub fn with_labels(
        collector: &'a MetricsCollector,
        name: impl Into<String>,
        labels: HashMap<String, String>,
    ) -> Self {
        Self {
            collector,
            name: name.into(),
            start: Instant::now(),
            labels,
        }
    }

    /// Add a label to the timer
    pub fn add_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }
}

impl<'a> Drop for TimerGuard<'a> {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        self.collector.record_with_labels(
            &self.name,
            MetricType::Duration(duration),
            std::mem::take(&mut self.labels),
        );
    }
}

/// Convenience function to create a timer guard
pub fn time_operation<'a>(collector: &'a MetricsCollector, name: &str) -> TimerGuard<'a> {
    TimerGuard::new(collector, name)
}
