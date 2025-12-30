//! Property-based tests for Watchdog module
//!
//! These tests verify the correctness properties of the watchdog system.

#[cfg(test)]
mod property_tests {
    use proptest::prelude::*;
    use std::time::{Duration, Instant};

    use crate::watchdog::shared_memory::{
        HeartbeatData, HEARTBEAT_INTERVAL_MS, HEARTBEAT_TIMEOUT_MS, SHARED_MEMORY_SIZE,
    };

    /// **Property 26: Watchdog Heartbeat Reliability**
    /// *For any* running NeuralFS main process, the heartbeat SHALL be sent to 
    /// shared memory at least once per heartbeat interval.
    /// **Validates: Process Supervisor**
    mod property_26_heartbeat_reliability {
        use super::*;

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(100))]

            /// Property: HeartbeatData timestamp updates correctly
            /// For any process ID, creating a HeartbeatData and updating it
            /// should result in a newer timestamp
            #[test]
            fn heartbeat_update_increases_timestamp(process_id in 1u32..u32::MAX) {
                let mut data = HeartbeatData::new(process_id);
                let initial_timestamp = data.last_heartbeat_ms;
                
                // Small delay to ensure timestamp changes
                std::thread::sleep(Duration::from_millis(1));
                
                data.update();
                
                // Timestamp should be greater or equal (equal if system clock resolution is low)
                prop_assert!(
                    data.last_heartbeat_ms >= initial_timestamp,
                    "Heartbeat timestamp should not decrease: {} -> {}",
                    initial_timestamp,
                    data.last_heartbeat_ms
                );
            }

            /// Property: HeartbeatData validation is consistent
            /// For any valid HeartbeatData, is_valid() should return true
            #[test]
            fn valid_heartbeat_passes_validation(process_id in 1u32..u32::MAX) {
                let data = HeartbeatData::new(process_id);
                
                prop_assert!(
                    data.is_valid(),
                    "Newly created HeartbeatData should be valid"
                );
                prop_assert_eq!(data.magic, HeartbeatData::MAGIC);
                prop_assert_eq!(data.version, HeartbeatData::VERSION);
                prop_assert_eq!(data.process_id, process_id);
            }

            /// Property: Timeout detection is accurate
            /// For any timeout value, a fresh heartbeat should not be timed out,
            /// and an old heartbeat should be timed out
            #[test]
            fn timeout_detection_accuracy(
                process_id in 1u32..u32::MAX,
                timeout_ms in 100u64..10000u64
            ) {
                let data = HeartbeatData::new(process_id);
                
                // Fresh heartbeat should not be timed out
                prop_assert!(
                    !data.is_timed_out(timeout_ms),
                    "Fresh heartbeat should not be timed out"
                );
                
                // Simulate old heartbeat
                let mut old_data = data;
                old_data.last_heartbeat_ms = HeartbeatData::current_timestamp_ms()
                    .saturating_sub(timeout_ms + 1000);
                
                prop_assert!(
                    old_data.is_timed_out(timeout_ms),
                    "Old heartbeat should be timed out"
                );
            }

            /// Property: HeartbeatData size is fixed
            /// The HeartbeatData struct should always be exactly SHARED_MEMORY_SIZE bytes
            #[test]
            fn heartbeat_data_size_is_fixed(process_id in 1u32..u32::MAX) {
                let data = HeartbeatData::new(process_id);
                let size = std::mem::size_of_val(&data);
                
                prop_assert_eq!(
                    size,
                    SHARED_MEMORY_SIZE,
                    "HeartbeatData size should be exactly {} bytes, got {}",
                    SHARED_MEMORY_SIZE,
                    size
                );
            }

            /// Property: Invalid magic number fails validation
            /// For any HeartbeatData with wrong magic number, is_valid() should return false
            #[test]
            fn invalid_magic_fails_validation(
                process_id in 1u32..u32::MAX,
                bad_magic in 0u32..u32::MAX
            ) {
                prop_assume!(bad_magic != HeartbeatData::MAGIC);
                
                let mut data = HeartbeatData::new(process_id);
                data.magic = bad_magic;
                
                prop_assert!(
                    !data.is_valid(),
                    "HeartbeatData with wrong magic should be invalid"
                );
            }

            /// Property: Invalid version fails validation
            /// For any HeartbeatData with wrong version, is_valid() should return false
            #[test]
            fn invalid_version_fails_validation(
                process_id in 1u32..u32::MAX,
                bad_version in 0u32..u32::MAX
            ) {
                prop_assume!(bad_version != HeartbeatData::VERSION);
                
                let mut data = HeartbeatData::new(process_id);
                data.version = bad_version;
                
                prop_assert!(
                    !data.is_valid(),
                    "HeartbeatData with wrong version should be invalid"
                );
            }
        }

        /// Property: Heartbeat interval timing
        /// Verifies that heartbeats can be sent within the expected interval
        #[test]
        fn heartbeat_interval_timing() {
            let mut data = HeartbeatData::new(std::process::id());
            let start = Instant::now();
            
            // Simulate multiple heartbeat cycles
            for _ in 0..5 {
                let before_update = data.last_heartbeat_ms;
                std::thread::sleep(Duration::from_millis(10)); // Small delay
                data.update();
                
                // Each update should increase or maintain timestamp
                assert!(
                    data.last_heartbeat_ms >= before_update,
                    "Heartbeat timestamp should not decrease"
                );
            }
            
            let elapsed = start.elapsed();
            
            // Total time should be reasonable (5 * 10ms + overhead)
            assert!(
                elapsed < Duration::from_millis(500),
                "Heartbeat updates should be fast, took {:?}",
                elapsed
            );
        }

        /// Property: Heartbeat data round-trip through bytes
        /// For any HeartbeatData, converting to bytes and back should preserve all fields
        #[test]
        fn heartbeat_data_byte_roundtrip() {
            use proptest::test_runner::{TestRunner, Config};
            
            let mut runner = TestRunner::new(Config::with_cases(100));
            
            runner.run(&(1u32..u32::MAX), |process_id| {
                let original = HeartbeatData::new(process_id);
                
                // Convert to bytes
                let bytes = unsafe {
                    std::slice::from_raw_parts(
                        &original as *const HeartbeatData as *const u8,
                        SHARED_MEMORY_SIZE,
                    )
                };
                
                // Convert back
                let restored = unsafe {
                    std::ptr::read(bytes.as_ptr() as *const HeartbeatData)
                };
                
                // Verify all fields match
                prop_assert_eq!(original.magic, restored.magic);
                prop_assert_eq!(original.version, restored.version);
                prop_assert_eq!(original.process_id, restored.process_id);
                prop_assert_eq!(original.last_heartbeat_ms, restored.last_heartbeat_ms);
                prop_assert_eq!(original.state_flags, restored.state_flags);
                
                Ok(())
            }).unwrap();
        }
    }

    /// Additional tests for WatchdogConfig
    mod watchdog_config_tests {
        use crate::watchdog::{WatchdogConfig, HEARTBEAT_INTERVAL_MS, HEARTBEAT_TIMEOUT_MS};

        #[test]
        fn default_config_has_valid_intervals() {
            let config = WatchdogConfig::default();
            
            // Timeout should be greater than check interval
            assert!(
                config.timeout_ms > config.check_interval_ms,
                "Timeout ({}) should be greater than check interval ({})",
                config.timeout_ms,
                config.check_interval_ms
            );
            
            // Check interval should match constant
            assert_eq!(config.check_interval_ms, HEARTBEAT_INTERVAL_MS);
            
            // Timeout should match constant
            assert_eq!(config.timeout_ms, HEARTBEAT_TIMEOUT_MS);
        }

        #[test]
        fn default_config_has_reasonable_restart_settings() {
            let config = WatchdogConfig::default();
            
            // Should have at least 1 restart attempt
            assert!(config.max_restart_attempts >= 1);
            
            // Restart delay should be positive
            assert!(config.restart_delay_ms > 0);
        }
    }
}
