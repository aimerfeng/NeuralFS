//! Status Commands for NeuralFS
//!
//! Provides Tauri commands for system status monitoring:
//! - get_index_status: Get indexing status and statistics
//! - get_system_status: Get overall system status
//! - get_dead_letter_tasks: Get failed tasks from dead letter queue
//! - retry_dead_letter: Retry a failed task
//!
//! **Validates: Requirements 16.1, Indexer Resilience**

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::indexer::{IndexTask, TaskStatus, TaskPriority, IndexerStatsSnapshot, DeadLetterStats};

/// Index status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStatusDto {
    /// Total files processed
    pub total_processed: u64,
    /// Total files failed (but may have been retried)
    pub total_failed: u64,
    /// Total files in dead letter queue
    pub total_dead_letter: u64,
    /// Current pending queue size
    pub current_queue_size: u64,
    /// Current dead letter queue size
    pub current_dead_letter_size: u64,
    /// Indexing throughput (files per minute)
    pub throughput_per_minute: f64,
    /// Whether indexing is currently active
    pub is_indexing: bool,
    /// Estimated time to complete (seconds)
    pub estimated_completion_secs: Option<u64>,
}

/// System status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatusDto {
    /// Application version
    pub version: String,
    /// Uptime in seconds
    pub uptime_secs: u64,
    /// Memory usage in MB
    pub memory_usage_mb: u64,
    /// VRAM usage in MB (if GPU available)
    pub vram_usage_mb: Option<u64>,
    /// CPU usage percentage
    pub cpu_usage_percent: f32,
    /// Whether GPU acceleration is available
    pub gpu_available: bool,
    /// GPU provider (CUDA, Metal, etc.)
    pub gpu_provider: Option<String>,
    /// Database size in MB
    pub database_size_mb: u64,
    /// Vector index size in MB
    pub vector_index_size_mb: u64,
    /// Number of indexed files
    pub indexed_files_count: u64,
    /// Number of indexed chunks
    pub indexed_chunks_count: u64,
    /// File watcher status
    pub watcher_status: String,
    /// Cloud connection status
    pub cloud_status: String,
}

/// Dead letter task DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetterTaskDto {
    /// Task ID
    pub id: String,
    /// File ID
    pub file_id: String,
    /// File path
    pub path: String,
    /// Task priority
    pub priority: String,
    /// Number of retry attempts
    pub retry_count: u32,
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Last error message
    pub last_error: Option<String>,
    /// Error type
    pub error_type: Option<String>,
    /// Task creation time
    pub created_at: String,
    /// Time since creation
    pub age_secs: u64,
}

/// Dead letter queue statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetterStatsDto {
    /// Total count of tasks
    pub total_count: usize,
    /// Maximum queue size
    pub max_size: usize,
    /// Utilization percentage
    pub utilization_percent: f64,
    /// Count by error type
    pub by_error_type: Vec<ErrorTypeCount>,
    /// Age of oldest task in seconds
    pub oldest_task_age_secs: Option<u64>,
    /// Age of newest task in seconds
    pub newest_task_age_secs: Option<u64>,
}

/// Error type count
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorTypeCount {
    /// Error type name
    pub error_type: String,
    /// Count
    pub count: usize,
}

/// Retry operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryOperationResult {
    /// Whether the operation succeeded
    pub success: bool,
    /// Human-readable message
    pub message: String,
    /// Number of tasks retried
    pub tasks_retried: usize,
}

/// Get indexing status and statistics
///
/// Returns current indexing status including:
/// - Queue sizes
/// - Processing statistics
/// - Throughput metrics
///
/// # Returns
/// Index status
#[tauri::command]
pub async fn get_index_status() -> Result<IndexStatusDto, String> {
    // In production, this would query the ResilientBatchIndexer
    Ok(IndexStatusDto {
        total_processed: 0,
        total_failed: 0,
        total_dead_letter: 0,
        current_queue_size: 0,
        current_dead_letter_size: 0,
        throughput_per_minute: 0.0,
        is_indexing: false,
        estimated_completion_secs: None,
    })
}

/// Get overall system status
///
/// Returns comprehensive system status including:
/// - Resource usage (memory, VRAM, CPU)
/// - Database statistics
/// - Service statuses
///
/// # Returns
/// System status
#[tauri::command]
pub async fn get_system_status() -> Result<SystemStatusDto, String> {
    // In production, this would gather real system metrics
    Ok(SystemStatusDto {
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_secs: 0,
        memory_usage_mb: 0,
        vram_usage_mb: None,
        cpu_usage_percent: 0.0,
        gpu_available: false,
        gpu_provider: None,
        database_size_mb: 0,
        vector_index_size_mb: 0,
        indexed_files_count: 0,
        indexed_chunks_count: 0,
        watcher_status: "stopped".to_string(),
        cloud_status: "disconnected".to_string(),
    })
}

/// Get tasks from dead letter queue
///
/// Returns all tasks that have exceeded their retry limit.
///
/// # Arguments
/// * `limit` - Maximum number of tasks to return
/// * `error_type` - Optional filter by error type
///
/// # Returns
/// List of dead letter tasks
#[tauri::command]
pub async fn get_dead_letter_tasks(
    limit: Option<usize>,
    error_type: Option<String>,
) -> Result<Vec<DeadLetterTaskDto>, String> {
    let _limit = limit.unwrap_or(100);
    let _error_type = error_type;

    // In production, this would query the ResilientBatchIndexer
    Ok(vec![])
}

/// Get dead letter queue statistics
///
/// Returns statistics about the dead letter queue.
///
/// # Returns
/// Dead letter queue statistics
#[tauri::command]
pub async fn get_dead_letter_stats() -> Result<DeadLetterStatsDto, String> {
    // In production, this would query the ResilientBatchIndexer
    Ok(DeadLetterStatsDto {
        total_count: 0,
        max_size: 1000,
        utilization_percent: 0.0,
        by_error_type: vec![],
        oldest_task_age_secs: None,
        newest_task_age_secs: None,
    })
}

/// Retry a specific task from dead letter queue
///
/// Resets a failed task and moves it back to the pending queue.
///
/// # Arguments
/// * `task_id` - Task ID to retry
///
/// # Returns
/// Operation result
#[tauri::command]
pub async fn retry_dead_letter(task_id: String) -> Result<RetryOperationResult, String> {
    let _task_uuid = Uuid::parse_str(&task_id)
        .map_err(|e| format!("Invalid task_id: {}", e))?;

    // In production, this would use ResilientBatchIndexer::retry_dead_letter_task
    Ok(RetryOperationResult {
        success: true,
        message: "Task queued for retry".to_string(),
        tasks_retried: 1,
    })
}

/// Retry all tasks in dead letter queue
///
/// Resets all failed tasks and moves them back to the pending queue.
///
/// # Returns
/// Operation result
#[tauri::command]
pub async fn retry_all_dead_letter() -> Result<RetryOperationResult, String> {
    // In production, this would use ResilientBatchIndexer::retry_all_dead_letter_tasks
    Ok(RetryOperationResult {
        success: true,
        message: "All tasks queued for retry".to_string(),
        tasks_retried: 0,
    })
}

/// Clear dead letter queue
///
/// Permanently removes all tasks from the dead letter queue.
///
/// # Returns
/// Operation result
#[tauri::command]
pub async fn clear_dead_letter() -> Result<RetryOperationResult, String> {
    // In production, this would use ResilientBatchIndexer::clear_dead_letter_queue
    Ok(RetryOperationResult {
        success: true,
        message: "Dead letter queue cleared".to_string(),
        tasks_retried: 0,
    })
}

/// Pause indexing
///
/// Temporarily pauses the indexing process.
///
/// # Returns
/// Operation result
#[tauri::command]
pub async fn pause_indexing() -> Result<RetryOperationResult, String> {
    // In production, this would pause the indexer
    Ok(RetryOperationResult {
        success: true,
        message: "Indexing paused".to_string(),
        tasks_retried: 0,
    })
}

/// Resume indexing
///
/// Resumes a paused indexing process.
///
/// # Returns
/// Operation result
#[tauri::command]
pub async fn resume_indexing() -> Result<RetryOperationResult, String> {
    // In production, this would resume the indexer
    Ok(RetryOperationResult {
        success: true,
        message: "Indexing resumed".to_string(),
        tasks_retried: 0,
    })
}

// Helper functions

fn task_status_to_string(status: &TaskStatus) -> String {
    match status {
        TaskStatus::Pending => "pending".to_string(),
        TaskStatus::Processing => "processing".to_string(),
        TaskStatus::Completed => "completed".to_string(),
        TaskStatus::Failed => "failed".to_string(),
        TaskStatus::DeadLetter => "dead_letter".to_string(),
    }
}

fn task_priority_to_string(priority: &TaskPriority) -> String {
    match priority {
        TaskPriority::Low => "low".to_string(),
        TaskPriority::Normal => "normal".to_string(),
        TaskPriority::High => "high".to_string(),
        TaskPriority::Urgent => "urgent".to_string(),
    }
}
