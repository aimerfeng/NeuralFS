//! Tests for the indexer module
//!
//! This module contains both unit tests and property-based tests for:
//! - Property 39: Exponential Backoff Correctness
//! - Property 40: Dead Letter Queue Bound
//! - Property 41: File Lock Retry Behavior
//! - Property 42: Task State Machine Validity

use super::*;
use proptest::prelude::*;
use std::path::PathBuf;
use uuid::Uuid;

// ============================================================================
// Unit Tests
// ============================================================================

#[test]
fn test_task_creation() {
    let file_id = Uuid::now_v7();
    let path = PathBuf::from("/test/file.txt");
    let task = IndexTask::new(file_id, path.clone(), TaskPriority::Normal);

    assert_eq!(task.file_id, file_id);
    assert_eq!(task.path, path);
    assert_eq!(task.priority, TaskPriority::Normal);
    assert_eq!(task.retry_count, 0);
    assert_eq!(task.max_retries, 5);
    assert_eq!(task.status, TaskStatus::Pending);
    assert!(task.next_retry_at.is_none());
    assert!(task.last_error.is_none());
}

#[test]
fn test_task_status_transitions() {
    // Valid transitions
    assert!(TaskStatus::Pending.can_transition_to(TaskStatus::Processing));
    assert!(TaskStatus::Processing.can_transition_to(TaskStatus::Completed));
    assert!(TaskStatus::Processing.can_transition_to(TaskStatus::Failed));
    assert!(TaskStatus::Processing.can_transition_to(TaskStatus::DeadLetter));
    assert!(TaskStatus::Failed.can_transition_to(TaskStatus::Pending));
    assert!(TaskStatus::DeadLetter.can_transition_to(TaskStatus::Pending));

    // Invalid transitions
    assert!(!TaskStatus::Pending.can_transition_to(TaskStatus::Completed));
    assert!(!TaskStatus::Pending.can_transition_to(TaskStatus::Failed));
    assert!(!TaskStatus::Completed.can_transition_to(TaskStatus::Pending));
    assert!(!TaskStatus::Completed.can_transition_to(TaskStatus::Processing));
    assert!(!TaskStatus::Failed.can_transition_to(TaskStatus::Completed));
}

#[test]
fn test_task_mark_failed() {
    let mut task = IndexTask::new(Uuid::now_v7(), PathBuf::from("/test"), TaskPriority::Normal);
    task.max_retries = 3;

    // First failure
    task.mark_failed(IndexError::Timeout);
    assert_eq!(task.retry_count, 1);
    assert_eq!(task.status, TaskStatus::Failed);
    assert!(task.next_retry_at.is_some());

    // Second failure
    task.status = TaskStatus::Processing; // Simulate retry
    task.mark_failed(IndexError::Timeout);
    assert_eq!(task.retry_count, 2);
    assert_eq!(task.status, TaskStatus::Failed);

    // Third failure - should go to dead letter
    task.status = TaskStatus::Processing;
    task.mark_failed(IndexError::Timeout);
    assert_eq!(task.retry_count, 3);
    assert_eq!(task.status, TaskStatus::DeadLetter);
}

#[test]
fn test_task_is_ready() {
    let mut task = IndexTask::new(Uuid::now_v7(), PathBuf::from("/test"), TaskPriority::Normal);

    // Pending task is ready
    assert!(task.is_ready());

    // Processing task is not ready
    task.status = TaskStatus::Processing;
    assert!(!task.is_ready());

    // Completed task is not ready
    task.status = TaskStatus::Completed;
    assert!(!task.is_ready());

    // DeadLetter task is not ready
    task.status = TaskStatus::DeadLetter;
    assert!(!task.is_ready());

    // Failed task with past retry time is ready
    task.status = TaskStatus::Failed;
    task.next_retry_at = Some(Instant::now() - Duration::from_secs(1));
    assert!(task.is_ready());

    // Failed task with future retry time is not ready
    task.next_retry_at = Some(Instant::now() + Duration::from_secs(100));
    assert!(!task.is_ready());
}

#[test]
fn test_error_is_retryable() {
    assert!(IndexError::FileLocked { path: PathBuf::new() }.is_retryable());
    assert!(IndexError::IoError { reason: "test".to_string() }.is_retryable());
    assert!(IndexError::Timeout.is_retryable());
    assert!(IndexError::StorageFailed { reason: "test".to_string() }.is_retryable());
    assert!(IndexError::EmbeddingFailed { reason: "test".to_string() }.is_retryable());

    assert!(!IndexError::FileNotFound { path: PathBuf::new() }.is_retryable());
    assert!(!IndexError::UnsupportedFileType { extension: "xyz".to_string() }.is_retryable());
    assert!(!IndexError::IndexCorrupted { reason: "test".to_string() }.is_retryable());
}

#[tokio::test]
async fn test_indexer_submit_and_collect() {
    let indexer = ResilientBatchIndexer::with_defaults();

    // Submit tasks
    let task1 = IndexTask::new(Uuid::now_v7(), PathBuf::from("/test1"), TaskPriority::Normal);
    let task2 = IndexTask::new(Uuid::now_v7(), PathBuf::from("/test2"), TaskPriority::High);
    let task3 = IndexTask::new(Uuid::now_v7(), PathBuf::from("/test3"), TaskPriority::Low);

    indexer.submit(task1).await.unwrap();
    indexer.submit(task2).await.unwrap();
    indexer.submit(task3).await.unwrap();

    assert_eq!(indexer.pending_count().await, 3);

    // Collect ready tasks - should be sorted by priority
    let ready = indexer.collect_ready_tasks().await;
    assert_eq!(ready.len(), 3);
    assert_eq!(ready[0].priority, TaskPriority::High);
    assert_eq!(ready[1].priority, TaskPriority::Normal);
    assert_eq!(ready[2].priority, TaskPriority::Low);
}

#[tokio::test]
async fn test_dead_letter_queue_operations() {
    let config = IndexerConfig {
        dead_letter_max_size: 5,
        ..Default::default()
    };
    let indexer = ResilientBatchIndexer::new(config);

    // Create a task and move it to dead letter
    let mut task = IndexTask::new(Uuid::now_v7(), PathBuf::from("/test"), TaskPriority::Normal);
    task.max_retries = 1;
    task.mark_failed(IndexError::Timeout);
    assert_eq!(task.status, TaskStatus::DeadLetter);

    let task_id = task.id;
    indexer.handle_failure(task.clone(), IndexError::Timeout).await;

    // Verify task is in dead letter queue
    assert_eq!(indexer.dead_letter_count().await, 1);
    let dlq_tasks = indexer.get_dead_letter_tasks().await;
    assert_eq!(dlq_tasks.len(), 1);

    // Retry the task
    indexer.retry_dead_letter_task(task_id).await.unwrap();
    assert_eq!(indexer.dead_letter_count().await, 0);
    assert_eq!(indexer.pending_count().await, 1);
}


// ============================================================================
// Property-Based Tests
// ============================================================================

proptest! {
    /// Property 39: Exponential Backoff Correctness
    /// 
    /// *For any* failed IndexTask with retry_count n, the retry delay SHALL be
    /// approximately 2^n seconds (with jitter), up to a maximum of 16 seconds.
    /// 
    /// **Validates: Indexer Resilience**
    #[test]
    fn prop_exponential_backoff_correctness(
        retry_count in 0u32..20,
        jitter_random in 0.0f64..1.0,
    ) {
        // Feature: neural-fs-core, Property 39: Exponential Backoff Correctness
        let delay = calculate_retry_delay_with_jitter(retry_count, jitter_random);
        
        // Calculate expected base delay (capped at 16 seconds)
        let capped_count = retry_count.min(4);
        let expected_base_secs = 1u64 << capped_count; // 1, 2, 4, 8, 16
        
        // Jitter factor should be between 0.75 and 1.25
        let jitter_factor = 0.75 + jitter_random * 0.5;
        let expected_delay_secs = expected_base_secs as f64 * jitter_factor;
        
        // Verify the delay is approximately correct (within 1ms tolerance for floating point)
        let actual_secs = delay.as_secs_f64();
        prop_assert!(
            (actual_secs - expected_delay_secs).abs() < 0.001,
            "Expected delay ~{:.3}s, got {:.3}s for retry_count={}, jitter={}",
            expected_delay_secs, actual_secs, retry_count, jitter_random
        );
        
        // Verify delay is within bounds: 0.75s to 20s (16 * 1.25)
        prop_assert!(delay >= Duration::from_millis(750), "Delay too short: {:?}", delay);
        prop_assert!(delay <= Duration::from_secs(20), "Delay too long: {:?}", delay);
        
        // Verify max delay cap (16 seconds base * 1.25 jitter = 20 seconds max)
        if retry_count >= 4 {
            // For retry_count >= 4, base should be capped at 16 seconds
            let min_expected = Duration::from_secs_f64(16.0 * 0.75);
            let max_expected = Duration::from_secs_f64(16.0 * 1.25);
            prop_assert!(delay >= min_expected, "Capped delay too short");
            prop_assert!(delay <= max_expected, "Capped delay too long");
        }
    }

    /// Property 40: Dead Letter Queue Bound
    /// 
    /// *For any* state of the indexer, the dead letter queue size SHALL not
    /// exceed dead_letter_max_size.
    /// 
    /// **Validates: Indexer Resilience**
    #[test]
    fn prop_dead_letter_queue_bound(
        max_size in 1usize..100,
        num_tasks in 0usize..200,
    ) {
        // Feature: neural-fs-core, Property 40: Dead Letter Queue Bound
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let config = IndexerConfig {
                dead_letter_max_size: max_size,
                ..Default::default()
            };
            let indexer = ResilientBatchIndexer::new(config);

            // Add tasks that will go to dead letter queue
            for i in 0..num_tasks {
                let mut task = IndexTask::new(
                    Uuid::now_v7(),
                    PathBuf::from(format!("/test/{}", i)),
                    TaskPriority::Normal,
                );
                task.max_retries = 0; // Will immediately go to dead letter
                task.status = TaskStatus::Processing;
                task.mark_failed(IndexError::Timeout);
                
                indexer.handle_failure(task, IndexError::Timeout).await;
            }

            // Verify queue size never exceeds max
            let dlq_size = indexer.dead_letter_count().await;
            prop_assert!(
                dlq_size <= max_size,
                "Dead letter queue size {} exceeds max {}",
                dlq_size, max_size
            );

            Ok(())
        })?;
    }

    /// Property 41: File Lock Retry Behavior
    /// 
    /// *For any* IndexTask that fails due to FileLocked error, the task SHALL
    /// be retried after file_lock_retry_secs seconds, not using exponential backoff.
    /// 
    /// **Validates: Indexer Resilience**
    #[test]
    fn prop_file_lock_retry_behavior(
        file_lock_retry_secs in 1u64..60,
        retry_count in 0u32..10,
    ) {
        // Feature: neural-fs-core, Property 41: File Lock Retry Behavior
        let mut task = IndexTask::new(
            Uuid::now_v7(),
            PathBuf::from("/test/locked_file.txt"),
            TaskPriority::Normal,
        );
        task.retry_count = retry_count;
        task.max_retries = 20; // High enough to not hit dead letter

        let before = Instant::now();
        task.mark_failed_file_locked(
            IndexError::FileLocked { path: PathBuf::from("/test/locked_file.txt") },
            file_lock_retry_secs,
        );
        let after = Instant::now();

        // Verify the task is scheduled for retry
        prop_assert_eq!(task.status, TaskStatus::Failed);
        prop_assert!(task.next_retry_at.is_some());

        // Verify the retry delay is the fixed file_lock_retry_secs, not exponential
        let next_retry = task.next_retry_at.unwrap();
        let expected_min = before + Duration::from_secs(file_lock_retry_secs);
        let expected_max = after + Duration::from_secs(file_lock_retry_secs) + Duration::from_millis(100);

        prop_assert!(
            next_retry >= expected_min && next_retry <= expected_max,
            "File lock retry should use fixed delay of {}s, not exponential backoff",
            file_lock_retry_secs
        );

        // Verify it's NOT using exponential backoff
        // If it were exponential, delay would be 2^retry_count seconds (with jitter)
        // For retry_count > 0, this would be different from file_lock_retry_secs
        if retry_count > 0 && file_lock_retry_secs != (1u64 << retry_count.min(4)) {
            let exponential_delay = Duration::from_secs(1u64 << retry_count.min(4));
            let actual_delay = next_retry.duration_since(before);
            
            // The actual delay should be closer to file_lock_retry_secs than to exponential
            let diff_from_fixed = (actual_delay.as_secs() as i64 - file_lock_retry_secs as i64).abs();
            let diff_from_exp = (actual_delay.as_secs() as i64 - exponential_delay.as_secs() as i64).abs();
            
            prop_assert!(
                diff_from_fixed <= diff_from_exp || diff_from_fixed <= 1,
                "Delay should be fixed {}s, not exponential {}s",
                file_lock_retry_secs, exponential_delay.as_secs()
            );
        }
    }

    /// Property 42: Task State Machine Validity
    /// 
    /// *For any* IndexTask, the status SHALL only transition through valid states:
    /// Pending → Processing → {Completed | Failed | DeadLetter}, and Failed → Pending (on retry).
    /// 
    /// **Validates: Indexer Resilience**
    #[test]
    fn prop_task_state_machine_validity(
        from_state in prop::sample::select(vec![
            TaskStatus::Pending,
            TaskStatus::Processing,
            TaskStatus::Completed,
            TaskStatus::Failed,
            TaskStatus::DeadLetter,
        ]),
        to_state in prop::sample::select(vec![
            TaskStatus::Pending,
            TaskStatus::Processing,
            TaskStatus::Completed,
            TaskStatus::Failed,
            TaskStatus::DeadLetter,
        ]),
    ) {
        // Feature: neural-fs-core, Property 42: Task State Machine Validity
        let mut task = IndexTask::new(
            Uuid::now_v7(),
            PathBuf::from("/test"),
            TaskPriority::Normal,
        );
        task.status = from_state;

        let result = task.transition_to(to_state);

        // Define valid transitions
        let is_valid_transition = match (from_state, to_state) {
            (TaskStatus::Pending, TaskStatus::Processing) => true,
            (TaskStatus::Processing, TaskStatus::Completed) => true,
            (TaskStatus::Processing, TaskStatus::Failed) => true,
            (TaskStatus::Processing, TaskStatus::DeadLetter) => true,
            (TaskStatus::Failed, TaskStatus::Pending) => true,
            (TaskStatus::DeadLetter, TaskStatus::Pending) => true,
            _ => false,
        };

        if is_valid_transition {
            prop_assert!(
                result.is_ok(),
                "Valid transition {:?} -> {:?} should succeed",
                from_state, to_state
            );
            prop_assert_eq!(task.status, to_state);
        } else {
            prop_assert!(
                result.is_err(),
                "Invalid transition {:?} -> {:?} should fail",
                from_state, to_state
            );
            // Status should remain unchanged on invalid transition
            prop_assert_eq!(task.status, from_state);
        }
    }
}

// ============================================================================
// Additional Unit Tests for Edge Cases
// ============================================================================

#[test]
fn test_retry_policy_file_lock_vs_exponential() {
    let policy = RetryPolicy::default();

    // File lock error should use fixed delay
    let file_lock_error = IndexError::FileLocked { path: PathBuf::from("/test") };
    let delay = policy.calculate_delay(5, &file_lock_error);
    assert_eq!(delay, Duration::from_millis(policy.file_lock_delay_ms));

    // Other errors should use exponential backoff
    let timeout_error = IndexError::Timeout;
    let delay = policy.calculate_delay(0, &timeout_error);
    // Should be around 1 second (base delay) with jitter
    assert!(delay >= Duration::from_millis(750));
    assert!(delay <= Duration::from_millis(1250));
}

#[test]
fn test_dead_letter_stats() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let config = IndexerConfig {
            dead_letter_max_size: 100,
            ..Default::default()
        };
        let indexer = ResilientBatchIndexer::new(config);

        // Add some tasks with different errors
        for i in 0..5 {
            let mut task = IndexTask::new(
                Uuid::now_v7(),
                PathBuf::from(format!("/test/{}", i)),
                TaskPriority::Normal,
            );
            task.max_retries = 0;
            task.status = TaskStatus::Processing;
            
            let error = if i % 2 == 0 {
                IndexError::Timeout
            } else {
                IndexError::FileLocked { path: PathBuf::from(format!("/test/{}", i)) }
            };
            task.mark_failed(error.clone());
            indexer.handle_failure(task, error).await;
        }

        let stats = indexer.dead_letter_stats().await;
        assert_eq!(stats.total_count, 5);
        assert!(!stats.is_at_capacity());
        assert!(stats.by_error_type.contains_key("Timeout"));
        assert!(stats.by_error_type.contains_key("FileLocked"));
    });
}

#[test]
fn test_file_access_check() {
    use file_access::*;
    use tempfile::NamedTempFile;

    // Create a temporary file
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path();

    // File should be accessible
    assert_eq!(check_file_accessible(path), FileAccessResult::Accessible);

    // Non-existent file
    let non_existent = PathBuf::from("/this/path/does/not/exist/file.txt");
    assert_eq!(check_file_accessible(&non_existent), FileAccessResult::NotFound);
}
