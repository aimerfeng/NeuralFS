//! Resilient Indexer Service for NeuralFS
//!
//! This module provides a fault-tolerant batch indexer with:
//! - Exponential backoff retry mechanism
//! - Dead letter queue for failed tasks
//! - File lock detection and special handling
//! - Task state machine with valid transitions

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use uuid::Uuid;

pub mod error;
#[cfg(test)]
mod tests;

pub use error::IndexError;

/// Task priority levels for indexing
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Urgent = 3,
}

impl Default for TaskPriority {
    fn default() -> Self {
        TaskPriority::Normal
    }
}

/// Task status representing the state machine states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    /// Waiting to be executed
    Pending,
    /// Currently being processed
    Processing,
    /// Successfully completed
    Completed,
    /// Failed but can be retried
    Failed,
    /// Moved to dead letter queue (no more retries)
    DeadLetter,
}

impl TaskStatus {
    /// Check if transition to the target status is valid
    pub fn can_transition_to(&self, target: TaskStatus) -> bool {
        match (self, target) {
            // Pending can go to Processing
            (TaskStatus::Pending, TaskStatus::Processing) => true,
            // Processing can go to Completed, Failed, or DeadLetter
            (TaskStatus::Processing, TaskStatus::Completed) => true,
            (TaskStatus::Processing, TaskStatus::Failed) => true,
            (TaskStatus::Processing, TaskStatus::DeadLetter) => true,
            // Failed can go back to Pending (on retry)
            (TaskStatus::Failed, TaskStatus::Pending) => true,
            // DeadLetter can go back to Pending (manual retry)
            (TaskStatus::DeadLetter, TaskStatus::Pending) => true,
            // All other transitions are invalid
            _ => false,
        }
    }
}


/// Enhanced index task with retry information
#[derive(Debug, Clone)]
pub struct IndexTask {
    /// Task unique identifier
    pub id: Uuid,
    /// File unique identifier
    pub file_id: Uuid,
    /// File path to index
    pub path: PathBuf,
    /// Task priority
    pub priority: TaskPriority,
    /// Task creation time
    pub created_at: Instant,
    /// Number of retry attempts
    pub retry_count: u32,
    /// Maximum retry attempts before moving to dead letter
    pub max_retries: u32,
    /// Next retry time (None means execute immediately)
    pub next_retry_at: Option<Instant>,
    /// Last error that caused failure
    pub last_error: Option<IndexError>,
    /// Current task status
    pub status: TaskStatus,
}

impl IndexTask {
    /// Create a new index task
    pub fn new(file_id: Uuid, path: PathBuf, priority: TaskPriority) -> Self {
        Self {
            id: Uuid::now_v7(),
            file_id,
            path,
            priority,
            created_at: Instant::now(),
            retry_count: 0,
            max_retries: 5,
            next_retry_at: None,
            last_error: None,
            status: TaskStatus::Pending,
        }
    }

    /// Create a new task with custom max retries
    pub fn with_max_retries(file_id: Uuid, path: PathBuf, priority: TaskPriority, max_retries: u32) -> Self {
        let mut task = Self::new(file_id, path, priority);
        task.max_retries = max_retries;
        task
    }

    /// Calculate retry delay using exponential backoff with jitter
    /// 
    /// Formula: base_delay * 2^retry_count * jitter_factor
    /// - Base delay: 1 second
    /// - Max delay: 16 seconds (capped at retry_count = 4)
    /// - Jitter: ±25% (factor between 0.75 and 1.25)
    pub fn calculate_retry_delay(&self) -> Duration {
        calculate_retry_delay(self.retry_count)
    }

    /// Mark task as failed and schedule retry
    pub fn mark_failed(&mut self, error: IndexError) {
        self.retry_count += 1;
        self.last_error = Some(error);

        if self.retry_count >= self.max_retries {
            self.status = TaskStatus::DeadLetter;
        } else {
            self.status = TaskStatus::Failed;
            self.next_retry_at = Some(Instant::now() + self.calculate_retry_delay());
        }
    }

    /// Mark task as failed with file lock error (uses fixed delay)
    pub fn mark_failed_file_locked(&mut self, error: IndexError, lock_retry_secs: u64) {
        self.retry_count += 1;
        self.last_error = Some(error);

        if self.retry_count >= self.max_retries {
            self.status = TaskStatus::DeadLetter;
        } else {
            self.status = TaskStatus::Failed;
            // Use fixed delay for file lock errors instead of exponential backoff
            self.next_retry_at = Some(Instant::now() + Duration::from_secs(lock_retry_secs));
        }
    }

    /// Check if task is ready to execute
    pub fn is_ready(&self) -> bool {
        match self.status {
            TaskStatus::Pending => true,
            TaskStatus::Failed => {
                self.next_retry_at
                    .map(|t| Instant::now() >= t)
                    .unwrap_or(true)
            }
            _ => false,
        }
    }

    /// Reset task for retry (from dead letter queue)
    pub fn reset_for_retry(&mut self) {
        self.retry_count = 0;
        self.status = TaskStatus::Pending;
        self.next_retry_at = None;
        self.last_error = None;
    }

    /// Transition to a new status with validation
    pub fn transition_to(&mut self, new_status: TaskStatus) -> Result<(), IndexError> {
        if self.status.can_transition_to(new_status) {
            self.status = new_status;
            Ok(())
        } else {
            Err(IndexError::InvalidStateTransition {
                from: self.status,
                to: new_status,
            })
        }
    }
}


/// Calculate retry delay using exponential backoff with jitter
/// 
/// This is a standalone function for testing purposes.
/// 
/// Formula: base_delay * 2^retry_count * jitter_factor
/// - Base delay: 1 second
/// - Max delay: 16 seconds (capped at retry_count = 4)
/// - Jitter: ±25% (factor between 0.75 and 1.25)
pub fn calculate_retry_delay(retry_count: u32) -> Duration {
    calculate_retry_delay_with_jitter(retry_count, rand::random::<f64>())
}

/// Calculate retry delay with explicit jitter value (for testing)
/// 
/// jitter_random should be a value between 0.0 and 1.0
pub fn calculate_retry_delay_with_jitter(retry_count: u32, jitter_random: f64) -> Duration {
    // Cap at 4 to get max 16 seconds (2^4 = 16)
    let capped_count = retry_count.min(4);
    let base_delay_secs = 1u64 << capped_count; // 1, 2, 4, 8, 16

    // Jitter factor: 0.75 to 1.25 (±25%)
    let jitter_factor = 0.75 + jitter_random * 0.5;
    
    Duration::from_secs_f64(base_delay_secs as f64 * jitter_factor)
}

/// Configuration for the resilient batch indexer
#[derive(Debug, Clone)]
pub struct IndexerConfig {
    /// Number of tasks to process in a batch
    pub batch_size: usize,
    /// Maximum concurrent tasks
    pub max_concurrent: usize,
    /// Task timeout in seconds
    pub task_timeout_secs: u64,
    /// Maximum size of dead letter queue
    pub dead_letter_max_size: usize,
    /// Fixed retry interval for file lock errors (seconds)
    pub file_lock_retry_secs: u64,
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            batch_size: 10,
            max_concurrent: 4,
            task_timeout_secs: 60,
            dead_letter_max_size: 1000,
            file_lock_retry_secs: 5,
        }
    }
}

/// Statistics for the indexer
#[derive(Debug, Default)]
pub struct IndexerStats {
    /// Total tasks successfully processed
    pub total_processed: AtomicU64,
    /// Total tasks that failed (but may have been retried)
    pub total_failed: AtomicU64,
    /// Total tasks moved to dead letter queue
    pub total_dead_letter: AtomicU64,
    /// Current pending queue size
    pub current_queue_size: AtomicU64,
    /// Current dead letter queue size
    pub current_dead_letter_size: AtomicU64,
}

impl IndexerStats {
    /// Create a snapshot of current stats
    pub fn snapshot(&self) -> IndexerStatsSnapshot {
        IndexerStatsSnapshot {
            total_processed: self.total_processed.load(Ordering::SeqCst),
            total_failed: self.total_failed.load(Ordering::SeqCst),
            total_dead_letter: self.total_dead_letter.load(Ordering::SeqCst),
            current_queue_size: self.current_queue_size.load(Ordering::SeqCst),
            current_dead_letter_size: self.current_dead_letter_size.load(Ordering::SeqCst),
        }
    }
}

/// Snapshot of indexer statistics
#[derive(Debug, Clone)]
pub struct IndexerStatsSnapshot {
    pub total_processed: u64,
    pub total_failed: u64,
    pub total_dead_letter: u64,
    pub current_queue_size: u64,
    pub current_dead_letter_size: u64,
}


/// Resilient batch indexer with retry mechanism and dead letter queue
pub struct ResilientBatchIndexer {
    /// Pending task queue
    pending_queue: Arc<Mutex<VecDeque<IndexTask>>>,
    /// Dead letter queue for tasks that exceeded max retries
    dead_letter_queue: Arc<Mutex<VecDeque<IndexTask>>>,
    /// Configuration
    config: IndexerConfig,
    /// Statistics
    stats: Arc<IndexerStats>,
    /// Shutdown flag
    shutdown: Arc<std::sync::atomic::AtomicBool>,
}

impl ResilientBatchIndexer {
    /// Create a new resilient batch indexer
    pub fn new(config: IndexerConfig) -> Self {
        Self {
            pending_queue: Arc::new(Mutex::new(VecDeque::new())),
            dead_letter_queue: Arc::new(Mutex::new(VecDeque::new())),
            config,
            stats: Arc::new(IndexerStats::default()),
            shutdown: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Create with default configuration
    pub fn with_defaults() -> Self {
        Self::new(IndexerConfig::default())
    }

    /// Get the configuration
    pub fn config(&self) -> &IndexerConfig {
        &self.config
    }

    /// Get statistics
    pub fn stats(&self) -> &Arc<IndexerStats> {
        &self.stats
    }

    /// Submit a new task for indexing
    pub async fn submit(&self, task: IndexTask) -> Result<(), IndexError> {
        let mut queue = self.pending_queue.lock().await;
        queue.push_back(task);
        self.stats.current_queue_size.store(queue.len() as u64, Ordering::SeqCst);
        Ok(())
    }

    /// Submit multiple tasks for indexing
    pub async fn submit_batch(&self, tasks: Vec<IndexTask>) -> Result<(), IndexError> {
        let mut queue = self.pending_queue.lock().await;
        for task in tasks {
            queue.push_back(task);
        }
        self.stats.current_queue_size.store(queue.len() as u64, Ordering::SeqCst);
        Ok(())
    }

    /// Get current pending queue size
    pub async fn pending_count(&self) -> usize {
        self.pending_queue.lock().await.len()
    }

    /// Get current dead letter queue size
    pub async fn dead_letter_count(&self) -> usize {
        self.dead_letter_queue.lock().await.len()
    }

    /// Collect tasks that are ready to execute
    pub async fn collect_ready_tasks(&self) -> Vec<IndexTask> {
        let mut queue = self.pending_queue.lock().await;
        let mut batch = Vec::with_capacity(self.config.batch_size);

        // Collect all tasks and sort by priority and retry time
        let mut tasks: Vec<_> = queue.drain(..).collect();
        tasks.sort_by(|a, b| {
            // Higher priority first
            match b.priority.cmp(&a.priority) {
                std::cmp::Ordering::Equal => {
                    // Same priority: earlier retry time first
                    a.next_retry_at.cmp(&b.next_retry_at)
                }
                other => other,
            }
        });

        // Select ready tasks up to batch size
        for task in tasks {
            if task.is_ready() && batch.len() < self.config.batch_size {
                batch.push(task);
            } else {
                queue.push_back(task);
            }
        }

        self.stats.current_queue_size.store(queue.len() as u64, Ordering::SeqCst);
        batch
    }

    /// Handle a successful task completion
    pub async fn handle_success(&self, task: IndexTask) {
        self.stats.total_processed.fetch_add(1, Ordering::SeqCst);
        tracing::debug!("Indexed successfully: {:?}", task.path);
    }

    /// Handle a failed task
    pub async fn handle_failure(&self, mut task: IndexTask, error: IndexError) {
        let is_file_locked = matches!(error, IndexError::FileLocked { .. });

        if is_file_locked {
            task.mark_failed_file_locked(error, self.config.file_lock_retry_secs);
        } else {
            task.mark_failed(error);
        }

        if task.status == TaskStatus::DeadLetter {
            self.move_to_dead_letter(task).await;
            self.stats.total_dead_letter.fetch_add(1, Ordering::SeqCst);
        } else {
            let mut queue = self.pending_queue.lock().await;
            queue.push_back(task);
            self.stats.current_queue_size.store(queue.len() as u64, Ordering::SeqCst);
            self.stats.total_failed.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Move a task to the dead letter queue
    async fn move_to_dead_letter(&self, task: IndexTask) {
        let mut dlq = self.dead_letter_queue.lock().await;

        // Enforce dead letter queue size limit
        while dlq.len() >= self.config.dead_letter_max_size {
            dlq.pop_front();
        }

        tracing::warn!(
            "Task moved to dead letter queue: {:?} (retries: {}, error: {:?})",
            task.path,
            task.retry_count,
            task.last_error
        );

        dlq.push_back(task);
        self.stats.current_dead_letter_size.store(dlq.len() as u64, Ordering::SeqCst);
    }

    /// Get all tasks in the dead letter queue
    pub async fn get_dead_letter_tasks(&self) -> Vec<IndexTask> {
        self.dead_letter_queue.lock().await.iter().cloned().collect()
    }

    /// Retry a specific task from the dead letter queue
    pub async fn retry_dead_letter_task(&self, task_id: Uuid) -> Result<(), IndexError> {
        let mut dlq = self.dead_letter_queue.lock().await;

        if let Some(pos) = dlq.iter().position(|t| t.id == task_id) {
            let mut task = dlq.remove(pos).unwrap();
            task.reset_for_retry();

            // Update dead letter queue size
            self.stats.current_dead_letter_size.store(dlq.len() as u64, Ordering::SeqCst);

            // Re-queue the task
            drop(dlq); // Release lock before acquiring pending queue lock
            let mut queue = self.pending_queue.lock().await;
            queue.push_back(task);
            self.stats.current_queue_size.store(queue.len() as u64, Ordering::SeqCst);

            Ok(())
        } else {
            Err(IndexError::TaskNotFound { task_id })
        }
    }

    /// Retry all tasks in the dead letter queue
    pub async fn retry_all_dead_letter_tasks(&self) -> usize {
        let mut dlq = self.dead_letter_queue.lock().await;
        let count = dlq.len();

        let tasks: Vec<_> = dlq.drain(..).map(|mut t| {
            t.reset_for_retry();
            t
        }).collect();

        self.stats.current_dead_letter_size.store(0, Ordering::SeqCst);
        drop(dlq);

        let mut queue = self.pending_queue.lock().await;
        for task in tasks {
            queue.push_back(task);
        }
        self.stats.current_queue_size.store(queue.len() as u64, Ordering::SeqCst);

        count
    }

    /// Clear the dead letter queue
    pub async fn clear_dead_letter_queue(&self) -> usize {
        let mut dlq = self.dead_letter_queue.lock().await;
        let count = dlq.len();
        dlq.clear();
        self.stats.current_dead_letter_size.store(0, Ordering::SeqCst);
        count
    }

    /// Signal shutdown
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }

    /// Check if shutdown was requested
    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst)
    }
}


/// File access checker with explicit handle cleanup
/// 
/// This module provides utilities for safely checking file accessibility
/// while ensuring file handles are properly released, especially important
/// on Windows where failed open attempts can sometimes briefly hold handles.
pub mod file_access {
    use std::path::Path;
    use std::fs::File;
    use super::IndexError;

    /// Result of a file accessibility check
    #[derive(Debug, Clone, PartialEq)]
    pub enum FileAccessResult {
        /// File is accessible and can be read
        Accessible,
        /// File does not exist
        NotFound,
        /// File is locked by another process
        Locked,
        /// Other IO error occurred
        Error(String),
    }

    /// Check if a file is accessible for reading
    /// 
    /// This function explicitly drops the file handle after checking,
    /// which is important on Windows where even failed open attempts
    /// can sometimes briefly hold handles, causing consecutive retry failures.
    pub fn check_file_accessible(path: &Path) -> FileAccessResult {
        // First check if file exists
        if !path.exists() {
            return FileAccessResult::NotFound;
        }

        // Try to open the file
        let result = File::open(path);
        
        // Explicitly handle the result and ensure handle is dropped
        match result {
            Ok(file) => {
                // Explicitly drop the file handle to release it immediately
                drop(file);
                FileAccessResult::Accessible
            }
            Err(e) => {
                // Even on error, ensure any partial handle state is cleaned up
                // by letting the error go out of scope
                match e.kind() {
                    std::io::ErrorKind::NotFound => FileAccessResult::NotFound,
                    std::io::ErrorKind::PermissionDenied => FileAccessResult::Locked,
                    // On Windows, sharing violations also indicate locked files
                    _ if e.raw_os_error() == Some(32) => FileAccessResult::Locked, // ERROR_SHARING_VIOLATION
                    _ if e.raw_os_error() == Some(33) => FileAccessResult::Locked, // ERROR_LOCK_VIOLATION
                    _ => FileAccessResult::Error(e.to_string()),
                }
            }
        }
    }

    /// Check file accessibility and convert to IndexError if not accessible
    pub fn check_file_or_error(path: &Path) -> Result<(), IndexError> {
        match check_file_accessible(path) {
            FileAccessResult::Accessible => Ok(()),
            FileAccessResult::NotFound => Err(IndexError::FileNotFound {
                path: path.to_path_buf(),
            }),
            FileAccessResult::Locked => Err(IndexError::FileLocked {
                path: path.to_path_buf(),
            }),
            FileAccessResult::Error(reason) => Err(IndexError::IoError { reason }),
        }
    }

    /// Async version of file accessibility check
    pub async fn check_file_accessible_async(path: &Path) -> FileAccessResult {
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || check_file_accessible(&path))
            .await
            .unwrap_or(FileAccessResult::Error("Task join error".to_string()))
    }

    /// Async version that returns IndexError
    pub async fn check_file_or_error_async(path: &Path) -> Result<(), IndexError> {
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || check_file_or_error(&path))
            .await
            .unwrap_or(Err(IndexError::IoError {
                reason: "Task join error".to_string(),
            }))
    }
}

/// Retry policy for different error types
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of retries
    pub max_retries: u32,
    /// Base delay for exponential backoff (milliseconds)
    pub base_delay_ms: u64,
    /// Maximum delay cap (milliseconds)
    pub max_delay_ms: u64,
    /// Fixed delay for file lock errors (milliseconds)
    pub file_lock_delay_ms: u64,
    /// Whether to use jitter
    pub use_jitter: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 5,
            base_delay_ms: 1000,
            max_delay_ms: 16000,
            file_lock_delay_ms: 5000,
            use_jitter: true,
        }
    }
}

impl RetryPolicy {
    /// Calculate delay for a given retry count and error type
    pub fn calculate_delay(&self, retry_count: u32, error: &IndexError) -> Duration {
        if error.is_file_locked() {
            // Use fixed delay for file lock errors
            Duration::from_millis(self.file_lock_delay_ms)
        } else {
            // Use exponential backoff for other errors
            self.calculate_exponential_delay(retry_count)
        }
    }

    /// Calculate exponential backoff delay
    fn calculate_exponential_delay(&self, retry_count: u32) -> Duration {
        // Calculate base delay with exponential growth
        let multiplier = 1u64 << retry_count.min(10); // Cap to prevent overflow
        let delay_ms = self.base_delay_ms.saturating_mul(multiplier);
        let capped_delay_ms = delay_ms.min(self.max_delay_ms);

        if self.use_jitter {
            // Add ±25% jitter
            let jitter_factor = 0.75 + rand::random::<f64>() * 0.5;
            Duration::from_millis((capped_delay_ms as f64 * jitter_factor) as u64)
        } else {
            Duration::from_millis(capped_delay_ms)
        }
    }

    /// Check if retry should be attempted
    pub fn should_retry(&self, retry_count: u32, error: &IndexError) -> bool {
        retry_count < self.max_retries && error.is_retryable()
    }
}


/// Dead letter queue management utilities
impl ResilientBatchIndexer {
    /// Get a specific task from the dead letter queue by ID
    pub async fn get_dead_letter_task(&self, task_id: Uuid) -> Option<IndexTask> {
        let dlq = self.dead_letter_queue.lock().await;
        dlq.iter().find(|t| t.id == task_id).cloned()
    }

    /// Get dead letter tasks filtered by error type
    pub async fn get_dead_letter_tasks_by_error<F>(&self, filter: F) -> Vec<IndexTask>
    where
        F: Fn(&IndexError) -> bool,
    {
        let dlq = self.dead_letter_queue.lock().await;
        dlq.iter()
            .filter(|t| t.last_error.as_ref().map(&filter).unwrap_or(false))
            .cloned()
            .collect()
    }

    /// Remove a specific task from the dead letter queue (permanently discard)
    pub async fn remove_dead_letter_task(&self, task_id: Uuid) -> Option<IndexTask> {
        let mut dlq = self.dead_letter_queue.lock().await;
        if let Some(pos) = dlq.iter().position(|t| t.id == task_id) {
            let task = dlq.remove(pos);
            self.stats.current_dead_letter_size.store(dlq.len() as u64, Ordering::SeqCst);
            task
        } else {
            None
        }
    }

    /// Get dead letter queue statistics
    pub async fn dead_letter_stats(&self) -> DeadLetterStats {
        let dlq = self.dead_letter_queue.lock().await;
        
        let mut by_error_type = std::collections::HashMap::new();
        let mut oldest_task: Option<Instant> = None;
        let mut newest_task: Option<Instant> = None;

        for task in dlq.iter() {
            // Count by error type
            let error_key = task.last_error.as_ref()
                .map(|e| error_type_key(e))
                .unwrap_or("Unknown".to_string());
            *by_error_type.entry(error_key).or_insert(0) += 1;

            // Track oldest and newest
            match oldest_task {
                None => oldest_task = Some(task.created_at),
                Some(t) if task.created_at < t => oldest_task = Some(task.created_at),
                _ => {}
            }
            match newest_task {
                None => newest_task = Some(task.created_at),
                Some(t) if task.created_at > t => newest_task = Some(task.created_at),
                _ => {}
            }
        }

        DeadLetterStats {
            total_count: dlq.len(),
            max_size: self.config.dead_letter_max_size,
            by_error_type,
            oldest_task_age: oldest_task.map(|t| t.elapsed()),
            newest_task_age: newest_task.map(|t| t.elapsed()),
        }
    }
}

/// Statistics about the dead letter queue
#[derive(Debug, Clone)]
pub struct DeadLetterStats {
    /// Total number of tasks in the dead letter queue
    pub total_count: usize,
    /// Maximum allowed size
    pub max_size: usize,
    /// Count of tasks by error type
    pub by_error_type: std::collections::HashMap<String, usize>,
    /// Age of the oldest task
    pub oldest_task_age: Option<Duration>,
    /// Age of the newest task
    pub newest_task_age: Option<Duration>,
}

impl DeadLetterStats {
    /// Check if the queue is at capacity
    pub fn is_at_capacity(&self) -> bool {
        self.total_count >= self.max_size
    }

    /// Get utilization percentage
    pub fn utilization_percent(&self) -> f64 {
        if self.max_size == 0 {
            0.0
        } else {
            (self.total_count as f64 / self.max_size as f64) * 100.0
        }
    }
}

/// Get a string key for an error type (for statistics)
fn error_type_key(error: &IndexError) -> String {
    match error {
        IndexError::FileNotFound { .. } => "FileNotFound".to_string(),
        IndexError::FileLocked { .. } => "FileLocked".to_string(),
        IndexError::UnsupportedFileType { .. } => "UnsupportedFileType".to_string(),
        IndexError::ContentExtractionFailed { .. } => "ContentExtractionFailed".to_string(),
        IndexError::EmbeddingFailed { .. } => "EmbeddingFailed".to_string(),
        IndexError::StorageFailed { .. } => "StorageFailed".to_string(),
        IndexError::IoError { .. } => "IoError".to_string(),
        IndexError::Timeout => "Timeout".to_string(),
        IndexError::TaskNotFound { .. } => "TaskNotFound".to_string(),
        IndexError::IndexCorrupted { .. } => "IndexCorrupted".to_string(),
        IndexError::QueueFull => "QueueFull".to_string(),
        IndexError::InvalidStateTransition { .. } => "InvalidStateTransition".to_string(),
    }
}
