//! File Watcher Module
//!
//! Provides file system monitoring with event deduplication and throttling.
//! Uses notify-rs for cross-platform file system events.

mod filter;
#[cfg(test)]
mod tests;

pub use filter::{DirectoryFilter, DirectoryFilterConfig, FilterResult, FilterReason, UserFilterRules};

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

use crate::core::error::{NeuralFSError, Result};

/// File system event types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileEvent {
    /// File was created
    Created(PathBuf),
    /// File was modified
    Modified(PathBuf),
    /// File was deleted
    Deleted(PathBuf),
    /// File was renamed (old_path, new_path)
    Renamed(PathBuf, PathBuf),
}

/// Batch of file events (deduplicated and throttled)
#[derive(Debug, Clone)]
pub struct EventBatch {
    /// Unique batch ID
    pub id: Uuid,
    /// Events in this batch
    pub events: Vec<FileEvent>,
    /// Timestamp when batch was created
    pub created_at: Instant,
}

impl EventBatch {
    fn new() -> Self {
        Self {
            id: Uuid::now_v7(),
            events: Vec::new(),
            created_at: Instant::now(),
        }
    }
}


/// Configuration for the FileWatcher
#[derive(Debug, Clone)]
pub struct FileWatcherConfig {
    /// Debounce duration for events (default: 100ms)
    pub debounce_duration: Duration,
    /// Maximum batch size before forcing flush
    pub max_batch_size: usize,
    /// Maximum time to wait before flushing batch (default: 500ms)
    pub max_batch_wait: Duration,
    /// Channel buffer size for events
    pub channel_buffer_size: usize,
}

impl Default for FileWatcherConfig {
    fn default() -> Self {
        Self {
            debounce_duration: Duration::from_millis(100),
            max_batch_size: 100,
            max_batch_wait: Duration::from_millis(500),
            channel_buffer_size: 1000,
        }
    }
}

/// Internal state for event deduplication
#[derive(Debug)]
struct EventState {
    /// Last event kind for this path
    last_event: EventKind,
    /// Timestamp of last event
    last_seen: Instant,
    /// Original path (for rename tracking)
    original_path: Option<PathBuf>,
}

/// File watcher with event deduplication and throttling
pub struct FileWatcher {
    /// Configuration
    config: FileWatcherConfig,
    /// Directory filter
    filter: Arc<DirectoryFilter>,
    /// Watched directories
    watched_dirs: Arc<RwLock<Vec<PathBuf>>>,
    /// Event state for deduplication
    event_states: Arc<RwLock<HashMap<PathBuf, EventState>>>,
    /// Current batch being built
    current_batch: Arc<RwLock<EventBatch>>,
    /// Sender for batched events
    batch_sender: mpsc::Sender<EventBatch>,
    /// Internal watcher handle
    _watcher: Option<RecommendedWatcher>,
    /// Shutdown signal
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl FileWatcher {
    /// Create a new FileWatcher with default configuration
    pub fn new(filter: DirectoryFilter) -> Result<(Self, mpsc::Receiver<EventBatch>)> {
        Self::with_config(filter, FileWatcherConfig::default())
    }

    /// Create a new FileWatcher with custom configuration
    pub fn with_config(
        filter: DirectoryFilter,
        config: FileWatcherConfig,
    ) -> Result<(Self, mpsc::Receiver<EventBatch>)> {
        let (batch_sender, batch_receiver) = mpsc::channel(config.channel_buffer_size);

        let watcher = Self {
            config,
            filter: Arc::new(filter),
            watched_dirs: Arc::new(RwLock::new(Vec::new())),
            event_states: Arc::new(RwLock::new(HashMap::new())),
            current_batch: Arc::new(RwLock::new(EventBatch::new())),
            batch_sender,
            _watcher: None,
            shutdown_tx: None,
        };

        Ok((watcher, batch_receiver))
    }


    /// Start watching directories
    pub async fn start(&mut self, directories: Vec<PathBuf>) -> Result<()> {
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        self.shutdown_tx = Some(shutdown_tx);

        // Create internal channel for raw events
        let (raw_tx, mut raw_rx) = mpsc::channel::<Event>(self.config.channel_buffer_size);

        // Create the notify watcher
        let watcher = notify::recommended_watcher(move |res: std::result::Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = raw_tx.blocking_send(event);
            }
        }).map_err(|e| NeuralFSError::WatcherError(e.to_string()))?;

        self._watcher = Some(watcher);

        // Watch all directories
        for dir in &directories {
            self.watch_directory(dir)?;
        }

        // Store watched directories
        {
            let mut watched = self.watched_dirs.write().await;
            *watched = directories;
        }

        // Spawn the event processing task
        let event_states = Arc::clone(&self.event_states);
        let current_batch = Arc::clone(&self.current_batch);
        let batch_sender = self.batch_sender.clone();
        let filter = Arc::clone(&self.filter);
        let config = self.config.clone();

        tokio::spawn(async move {
            let mut flush_interval = tokio::time::interval(config.max_batch_wait);

            loop {
                tokio::select! {
                    // Handle shutdown
                    _ = shutdown_rx.recv() => {
                        // Flush remaining events
                        Self::flush_batch(&current_batch, &batch_sender).await;
                        break;
                    }

                    // Handle raw events
                    Some(event) = raw_rx.recv() => {
                        Self::process_raw_event(
                            event,
                            &event_states,
                            &current_batch,
                            &batch_sender,
                            &filter,
                            &config,
                        ).await;
                    }

                    // Periodic flush
                    _ = flush_interval.tick() => {
                        Self::flush_batch(&current_batch, &batch_sender).await;
                    }
                }
            }
        });

        Ok(())
    }

    /// Watch a single directory
    fn watch_directory(&mut self, path: &Path) -> Result<()> {
        if let Some(ref mut watcher) = self._watcher {
            watcher
                .watch(path, RecursiveMode::Recursive)
                .map_err(|e| NeuralFSError::WatcherError(format!("Failed to watch {:?}: {}", path, e)))?;
        }
        Ok(())
    }

    /// Stop watching and cleanup
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }
        self._watcher = None;
        Ok(())
    }


    /// Process a raw notify event with deduplication
    async fn process_raw_event(
        event: Event,
        event_states: &Arc<RwLock<HashMap<PathBuf, EventState>>>,
        current_batch: &Arc<RwLock<EventBatch>>,
        batch_sender: &mpsc::Sender<EventBatch>,
        filter: &Arc<DirectoryFilter>,
        config: &FileWatcherConfig,
    ) {
        let now = Instant::now();

        for path in event.paths {
            // Check if path should be filtered
            if let FilterResult::Exclude(_) = filter.should_filter(&path) {
                continue;
            }

            let file_event = Self::convert_event(&event.kind, &path, event_states).await;

            if let Some(file_event) = file_event {
                // Check for deduplication
                let should_add = {
                    let mut states = event_states.write().await;
                    
                    if let Some(state) = states.get(&path) {
                        // Deduplicate if same event within debounce window
                        if state.last_event == event.kind 
                            && now.duration_since(state.last_seen) < config.debounce_duration 
                        {
                            false
                        } else {
                            states.insert(path.clone(), EventState {
                                last_event: event.kind.clone(),
                                last_seen: now,
                                original_path: None,
                            });
                            true
                        }
                    } else {
                        states.insert(path.clone(), EventState {
                            last_event: event.kind.clone(),
                            last_seen: now,
                            original_path: None,
                        });
                        true
                    }
                };

                if should_add {
                    let mut batch = current_batch.write().await;
                    batch.events.push(file_event);

                    // Flush if batch is full
                    if batch.events.len() >= config.max_batch_size {
                        drop(batch);
                        Self::flush_batch(current_batch, batch_sender).await;
                    }
                }
            }
        }
    }

    /// Convert notify event kind to our FileEvent
    async fn convert_event(
        kind: &EventKind,
        path: &Path,
        event_states: &Arc<RwLock<HashMap<PathBuf, EventState>>>,
    ) -> Option<FileEvent> {
        match kind {
            EventKind::Create(_) => Some(FileEvent::Created(path.to_path_buf())),
            EventKind::Modify(_) => Some(FileEvent::Modified(path.to_path_buf())),
            EventKind::Remove(_) => Some(FileEvent::Deleted(path.to_path_buf())),
            EventKind::Rename(rename_mode) => {
                use notify::event::RenameMode;
                match rename_mode {
                    RenameMode::From => {
                        // Store the original path for later matching
                        let mut states = event_states.write().await;
                        states.insert(path.to_path_buf(), EventState {
                            last_event: kind.clone(),
                            last_seen: Instant::now(),
                            original_path: Some(path.to_path_buf()),
                        });
                        None // Wait for the "To" event
                    }
                    RenameMode::To => {
                        // Try to find the matching "From" event
                        let states = event_states.read().await;
                        for (old_path, state) in states.iter() {
                            if let Some(ref orig) = state.original_path {
                                if matches!(state.last_event, EventKind::Rename(RenameMode::From)) {
                                    return Some(FileEvent::Renamed(orig.clone(), path.to_path_buf()));
                                }
                            }
                        }
                        // No matching "From" found, treat as create
                        Some(FileEvent::Created(path.to_path_buf()))
                    }
                    RenameMode::Both => {
                        // Some platforms provide both paths at once
                        // This shouldn't happen with single path, but handle it
                        Some(FileEvent::Modified(path.to_path_buf()))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }


    /// Flush the current batch
    async fn flush_batch(
        current_batch: &Arc<RwLock<EventBatch>>,
        batch_sender: &mpsc::Sender<EventBatch>,
    ) {
        let batch = {
            let mut batch = current_batch.write().await;
            if batch.events.is_empty() {
                return;
            }
            std::mem::replace(&mut *batch, EventBatch::new())
        };

        let _ = batch_sender.send(batch).await;
    }

    /// Add a directory to watch
    pub async fn add_watch(&mut self, path: &Path) -> Result<()> {
        self.watch_directory(path)?;
        let mut watched = self.watched_dirs.write().await;
        if !watched.contains(&path.to_path_buf()) {
            watched.push(path.to_path_buf());
        }
        Ok(())
    }

    /// Remove a directory from watch
    pub async fn remove_watch(&mut self, path: &Path) -> Result<()> {
        if let Some(ref mut watcher) = self._watcher {
            let _ = watcher.unwatch(path);
        }
        let mut watched = self.watched_dirs.write().await;
        watched.retain(|p| p != path);
        Ok(())
    }

    /// Get list of watched directories
    pub async fn watched_directories(&self) -> Vec<PathBuf> {
        self.watched_dirs.read().await.clone()
    }

    /// Clear event state (useful for testing)
    pub async fn clear_state(&self) {
        let mut states = self.event_states.write().await;
        states.clear();
    }

    /// Get current batch size (useful for testing)
    pub async fn current_batch_size(&self) -> usize {
        self.current_batch.read().await.events.len()
    }
}

impl Drop for FileWatcher {
    fn drop(&mut self) {
        // Cleanup is handled by stop()
    }
}

/// Builder for FileWatcher
pub struct FileWatcherBuilder {
    config: FileWatcherConfig,
    filter_config: DirectoryFilterConfig,
}

impl FileWatcherBuilder {
    pub fn new() -> Self {
        Self {
            config: FileWatcherConfig::default(),
            filter_config: DirectoryFilterConfig::default(),
        }
    }

    pub fn debounce_duration(mut self, duration: Duration) -> Self {
        self.config.debounce_duration = duration;
        self
    }

    pub fn max_batch_size(mut self, size: usize) -> Self {
        self.config.max_batch_size = size;
        self
    }

    pub fn max_batch_wait(mut self, duration: Duration) -> Self {
        self.config.max_batch_wait = duration;
        self
    }

    pub fn filter_config(mut self, config: DirectoryFilterConfig) -> Self {
        self.filter_config = config;
        self
    }

    pub fn build(self) -> Result<(FileWatcher, mpsc::Receiver<EventBatch>)> {
        let filter = DirectoryFilter::new(self.filter_config)?;
        FileWatcher::with_config(filter, self.config)
    }
}

impl Default for FileWatcherBuilder {
    fn default() -> Self {
        Self::new()
    }
}
