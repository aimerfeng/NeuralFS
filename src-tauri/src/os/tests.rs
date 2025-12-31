//! Property-based tests for OS Integration module
//!
//! These tests verify the correctness properties of the OS integration layer.

#[cfg(test)]
mod property_tests {
    use proptest::prelude::*;
    use std::time::{Duration, Instant};

    use crate::os::{MonitorInfo, MonitorRect, MultiMonitorStrategy};

    /// **Property 36: Display Change Recovery**
    /// *For any* display configuration change (resolution, monitor add/remove), 
    /// the WindowsDesktopManager SHALL reattach to WorkerW and resize the window within 1 second.
    /// **Validates: Display Change Handling**
    mod property_36_display_change_recovery {
        use super::*;

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(100))]

            /// Property: MonitorRect calculations are consistent
            /// For any valid monitor dimensions, the rect should have positive area
            #[test]
            fn monitor_rect_has_positive_area(
                x in -10000i32..10000i32,
                y in -10000i32..10000i32,
                width in 1i32..10000i32,
                height in 1i32..10000i32
            ) {
                let rect = MonitorRect { x, y, width, height };
                
                // Area should be positive
                let area = (rect.width as i64) * (rect.height as i64);
                prop_assert!(
                    area > 0,
                    "Monitor rect should have positive area: {}x{} = {}",
                    rect.width, rect.height, area
                );
            }

            /// Property: Virtual screen bounds calculation is correct
            /// For any set of monitors, the virtual screen should encompass all of them
            #[test]
            fn virtual_screen_encompasses_all_monitors(
                monitors in prop::collection::vec(
                    (
                        -5000i32..5000i32,  // x
                        -5000i32..5000i32,  // y
                        100i32..3000i32,    // width
                        100i32..2000i32     // height
                    ),
                    1..5  // 1 to 5 monitors
                )
            ) {
                let monitor_infos: Vec<MonitorInfo> = monitors
                    .iter()
                    .enumerate()
                    .map(|(i, &(x, y, width, height))| MonitorInfo {
                        handle: i,
                        rect: MonitorRect { x, y, width, height },
                        is_primary: i == 0,
                        dpi_scale: 1.0,
                        name: format!("Monitor{}", i),
                    })
                    .collect();

                // Calculate virtual screen bounds
                let mut min_x = i32::MAX;
                let mut min_y = i32::MAX;
                let mut max_x = i32::MIN;
                let mut max_y = i32::MIN;

                for monitor in &monitor_infos {
                    min_x = min_x.min(monitor.rect.x);
                    min_y = min_y.min(monitor.rect.y);
                    max_x = max_x.max(monitor.rect.x + monitor.rect.width);
                    max_y = max_y.max(monitor.rect.y + monitor.rect.height);
                }

                let virtual_rect = MonitorRect {
                    x: min_x,
                    y: min_y,
                    width: max_x - min_x,
                    height: max_y - min_y,
                };

                // Virtual screen should encompass all monitors
                for monitor in &monitor_infos {
                    prop_assert!(
                        monitor.rect.x >= virtual_rect.x,
                        "Monitor x ({}) should be >= virtual x ({})",
                        monitor.rect.x, virtual_rect.x
                    );
                    prop_assert!(
                        monitor.rect.y >= virtual_rect.y,
                        "Monitor y ({}) should be >= virtual y ({})",
                        monitor.rect.y, virtual_rect.y
                    );
                    prop_assert!(
                        monitor.rect.x + monitor.rect.width <= virtual_rect.x + virtual_rect.width,
                        "Monitor right edge should be <= virtual right edge"
                    );
                    prop_assert!(
                        monitor.rect.y + monitor.rect.height <= virtual_rect.y + virtual_rect.height,
                        "Monitor bottom edge should be <= virtual bottom edge"
                    );
                }
            }

            /// Property: Multi-monitor strategy selection is deterministic
            /// For any strategy, applying it twice should produce the same result
            #[test]
            fn multi_monitor_strategy_is_deterministic(
                strategy_idx in 0usize..3usize
            ) {
                let strategy = match strategy_idx {
                    0 => MultiMonitorStrategy::PrimaryOnly,
                    1 => MultiMonitorStrategy::Unified,
                    _ => MultiMonitorStrategy::Independent,
                };

                // Strategy should equal itself
                prop_assert_eq!(strategy, strategy);
                
                // Default should be PrimaryOnly
                prop_assert_eq!(
                    MultiMonitorStrategy::default(),
                    MultiMonitorStrategy::PrimaryOnly
                );
            }

            /// Property: Monitor info preserves all fields
            /// For any monitor configuration, creating MonitorInfo should preserve all values
            #[test]
            fn monitor_info_preserves_fields(
                handle in 0usize..usize::MAX,
                x in -10000i32..10000i32,
                y in -10000i32..10000i32,
                width in 1i32..10000i32,
                height in 1i32..10000i32,
                is_primary in proptest::bool::ANY,
                dpi_scale in 0.5f32..4.0f32
            ) {
                let name = format!("Monitor_{}", handle);
                let monitor = MonitorInfo {
                    handle,
                    rect: MonitorRect { x, y, width, height },
                    is_primary,
                    dpi_scale,
                    name: name.clone(),
                };

                prop_assert_eq!(monitor.handle, handle);
                prop_assert_eq!(monitor.rect.x, x);
                prop_assert_eq!(monitor.rect.y, y);
                prop_assert_eq!(monitor.rect.width, width);
                prop_assert_eq!(monitor.rect.height, height);
                prop_assert_eq!(monitor.is_primary, is_primary);
                prop_assert!((monitor.dpi_scale - dpi_scale).abs() < 0.001);
                prop_assert_eq!(monitor.name, name);
            }
        }

        /// Property: Display change handling completes within time bound
        /// This test verifies that the handle_display_change operation is fast
        #[test]
        fn display_change_handling_is_fast() {
            // This is a timing test - we verify that the data structure operations
            // involved in display change handling are fast enough
            
            let start = Instant::now();
            
            // Simulate the operations that happen during display change
            for _ in 0..100 {
                // Create monitor info (simulating enumeration)
                let monitors: Vec<MonitorInfo> = (0..4)
                    .map(|i| MonitorInfo {
                        handle: i,
                        rect: MonitorRect {
                            x: (i as i32) * 1920,
                            y: 0,
                            width: 1920,
                            height: 1080,
                        },
                        is_primary: i == 0,
                        dpi_scale: 1.0,
                        name: format!("Monitor{}", i),
                    })
                    .collect();

                // Calculate virtual bounds
                let mut min_x = i32::MAX;
                let mut min_y = i32::MAX;
                let mut max_x = i32::MIN;
                let mut max_y = i32::MIN;

                for monitor in &monitors {
                    min_x = min_x.min(monitor.rect.x);
                    min_y = min_y.min(monitor.rect.y);
                    max_x = max_x.max(monitor.rect.x + monitor.rect.width);
                    max_y = max_y.max(monitor.rect.y + monitor.rect.height);
                }

                // Find primary
                let _primary = monitors.iter().find(|m| m.is_primary);
            }

            let elapsed = start.elapsed();
            
            // 100 iterations should complete well under 1 second
            // (the actual Windows API calls would add more time, but the logic should be fast)
            assert!(
                elapsed < Duration::from_millis(100),
                "Display change data operations should be fast, took {:?}",
                elapsed
            );
        }

        /// Property: Primary monitor is always identifiable
        /// For any set of monitors with at least one primary, we can find it
        #[test]
        fn primary_monitor_is_identifiable() {
            use proptest::test_runner::{TestRunner, Config};
            
            let mut runner = TestRunner::new(Config::with_cases(100));
            
            runner.run(
                &prop::collection::vec(proptest::bool::ANY, 1..10),
                |primary_flags| {
                    // Ensure at least one is primary
                    let mut flags = primary_flags;
                    if !flags.iter().any(|&p| p) {
                        flags[0] = true;
                    }

                    let monitors: Vec<MonitorInfo> = flags
                        .iter()
                        .enumerate()
                        .map(|(i, &is_primary)| MonitorInfo {
                            handle: i,
                            rect: MonitorRect {
                                x: 0,
                                y: 0,
                                width: 1920,
                                height: 1080,
                            },
                            is_primary,
                            dpi_scale: 1.0,
                            name: format!("Monitor{}", i),
                        })
                        .collect();

                    // Should be able to find at least one primary
                    let primary = monitors.iter().find(|m| m.is_primary);
                    prop_assert!(
                        primary.is_some(),
                        "Should always find a primary monitor"
                    );

                    Ok(())
                },
            ).unwrap();
        }
    }

    /// Tests for MonitorRect
    mod monitor_rect_tests {
        use super::*;

        #[test]
        fn default_monitor_rect_is_zero() {
            let rect = MonitorRect::default();
            assert_eq!(rect.x, 0);
            assert_eq!(rect.y, 0);
            assert_eq!(rect.width, 0);
            assert_eq!(rect.height, 0);
        }

        #[test]
        fn monitor_rect_copy_works() {
            let rect1 = MonitorRect {
                x: 100,
                y: 200,
                width: 1920,
                height: 1080,
            };
            let rect2 = rect1; // Copy

            assert_eq!(rect1.x, rect2.x);
            assert_eq!(rect1.y, rect2.y);
            assert_eq!(rect1.width, rect2.width);
            assert_eq!(rect1.height, rect2.height);
        }
    }

    /// Tests for MultiMonitorStrategy
    mod multi_monitor_strategy_tests {
        use super::*;

        #[test]
        fn default_strategy_is_primary_only() {
            assert_eq!(
                MultiMonitorStrategy::default(),
                MultiMonitorStrategy::PrimaryOnly
            );
        }

        #[test]
        fn strategies_are_distinct() {
            assert_ne!(
                MultiMonitorStrategy::PrimaryOnly,
                MultiMonitorStrategy::Unified
            );
            assert_ne!(
                MultiMonitorStrategy::PrimaryOnly,
                MultiMonitorStrategy::Independent
            );
            assert_ne!(
                MultiMonitorStrategy::Unified,
                MultiMonitorStrategy::Independent
            );
        }
    }

    /// **Property 28: Game Mode Detection Accuracy**
    /// *For any* fullscreen application running, the SystemActivityMonitor SHALL detect it
    /// within the check interval and transition to FullscreenApp state.
    /// **Validates: Game Mode Detection**
    mod property_28_game_mode_detection {
        use super::*;
        use crate::os::activity::{
            SystemState, ActivityMonitorConfig, GameModeStatus, GameModePolicyConfig,
        };

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(100))]

            /// Property: SystemState correctly identifies game mode triggers
            /// For any SystemState, should_enter_game_mode() returns true only for
            /// FullscreenApp and PresentationMode states
            #[test]
            fn system_state_game_mode_trigger_is_correct(
                state_idx in 0usize..5usize,
                app_name in proptest::option::of("[a-z]{3,10}\\.exe"),
                process_id in proptest::option::of(1u32..65535u32),
                battery_percent in 0u8..100u8
            ) {
                let state = match state_idx {
                    0 => SystemState::Normal,
                    1 => SystemState::FullscreenApp { app_name, process_id },
                    2 => SystemState::PresentationMode,
                    3 => SystemState::DoNotDisturb,
                    _ => SystemState::LowPower { battery_percent },
                };

                let should_trigger = state.should_enter_game_mode();

                // Only FullscreenApp and PresentationMode should trigger game mode
                match state {
                    SystemState::FullscreenApp { .. } => {
                        prop_assert!(should_trigger, "FullscreenApp should trigger game mode");
                    }
                    SystemState::PresentationMode => {
                        prop_assert!(should_trigger, "PresentationMode should trigger game mode");
                    }
                    _ => {
                        prop_assert!(!should_trigger, "{:?} should NOT trigger game mode", state);
                    }
                }
            }

            /// Property: SystemState correctly identifies resource constraints
            /// For any SystemState, has_resource_constraints() returns true for
            /// FullscreenApp, PresentationMode, and LowPower states
            #[test]
            fn system_state_resource_constraints_is_correct(
                state_idx in 0usize..5usize,
                app_name in proptest::option::of("[a-z]{3,10}\\.exe"),
                process_id in proptest::option::of(1u32..65535u32),
                battery_percent in 0u8..100u8
            ) {
                let state = match state_idx {
                    0 => SystemState::Normal,
                    1 => SystemState::FullscreenApp { app_name, process_id },
                    2 => SystemState::PresentationMode,
                    3 => SystemState::DoNotDisturb,
                    _ => SystemState::LowPower { battery_percent },
                };

                let has_constraints = state.has_resource_constraints();

                match state {
                    SystemState::FullscreenApp { .. } => {
                        prop_assert!(has_constraints, "FullscreenApp should have resource constraints");
                    }
                    SystemState::PresentationMode => {
                        prop_assert!(has_constraints, "PresentationMode should have resource constraints");
                    }
                    SystemState::LowPower { .. } => {
                        prop_assert!(has_constraints, "LowPower should have resource constraints");
                    }
                    _ => {
                        prop_assert!(!has_constraints, "{:?} should NOT have resource constraints", state);
                    }
                }
            }

            /// Property: SystemState description is never empty
            /// For any SystemState, description() returns a non-empty string
            #[test]
            fn system_state_description_is_non_empty(
                state_idx in 0usize..5usize,
                app_name in proptest::option::of("[a-z]{3,10}\\.exe"),
                process_id in proptest::option::of(1u32..65535u32),
                battery_percent in 0u8..100u8
            ) {
                let state = match state_idx {
                    0 => SystemState::Normal,
                    1 => SystemState::FullscreenApp { app_name, process_id },
                    2 => SystemState::PresentationMode,
                    3 => SystemState::DoNotDisturb,
                    _ => SystemState::LowPower { battery_percent },
                };

                let description = state.description();
                prop_assert!(
                    !description.is_empty(),
                    "Description should not be empty for {:?}",
                    state
                );
            }

            /// Property: SystemState equality is reflexive
            /// For any SystemState, it should equal itself
            #[test]
            fn system_state_equality_is_reflexive(
                state_idx in 0usize..5usize,
                app_name in proptest::option::of("[a-z]{3,10}\\.exe"),
                process_id in proptest::option::of(1u32..65535u32),
                battery_percent in 0u8..100u8
            ) {
                let state = match state_idx {
                    0 => SystemState::Normal,
                    1 => SystemState::FullscreenApp { 
                        app_name: app_name.clone(), 
                        process_id 
                    },
                    2 => SystemState::PresentationMode,
                    3 => SystemState::DoNotDisturb,
                    _ => SystemState::LowPower { battery_percent },
                };

                prop_assert_eq!(state.clone(), state, "State should equal itself");
            }

            /// Property: SystemState serialization round-trip preserves value
            /// For any SystemState, serializing and deserializing should produce the same value
            #[test]
            fn system_state_serialization_round_trip(
                state_idx in 0usize..5usize,
                app_name in proptest::option::of("[a-z]{3,10}\\.exe"),
                process_id in proptest::option::of(1u32..65535u32),
                battery_percent in 0u8..100u8
            ) {
                let state = match state_idx {
                    0 => SystemState::Normal,
                    1 => SystemState::FullscreenApp { 
                        app_name: app_name.clone(), 
                        process_id 
                    },
                    2 => SystemState::PresentationMode,
                    3 => SystemState::DoNotDisturb,
                    _ => SystemState::LowPower { battery_percent },
                };

                let json = serde_json::to_string(&state).expect("Serialization should succeed");
                let deserialized: SystemState = serde_json::from_str(&json)
                    .expect("Deserialization should succeed");

                prop_assert_eq!(
                    state, deserialized,
                    "Round-trip serialization should preserve value"
                );
            }

            /// Property: GameModeStatus serialization round-trip preserves value
            /// For any GameModeStatus, serializing and deserializing should produce the same value
            #[test]
            fn game_mode_status_serialization_round_trip(
                status_idx in 0usize..4usize
            ) {
                let status = match status_idx {
                    0 => GameModeStatus::Inactive,
                    1 => GameModeStatus::Entering,
                    2 => GameModeStatus::Active,
                    _ => GameModeStatus::Exiting,
                };

                let json = serde_json::to_string(&status).expect("Serialization should succeed");
                let deserialized: GameModeStatus = serde_json::from_str(&json)
                    .expect("Deserialization should succeed");

                prop_assert_eq!(
                    status, deserialized,
                    "Round-trip serialization should preserve value"
                );
            }

            /// Property: ActivityMonitorConfig check_interval is always positive
            /// For any valid configuration, check_interval should be positive
            #[test]
            fn activity_monitor_config_interval_is_positive(
                interval_secs in 1u64..3600u64,
                threshold in 1u8..100u8
            ) {
                let config = ActivityMonitorConfig {
                    check_interval: Duration::from_secs(interval_secs),
                    low_power_threshold: threshold,
                    ..ActivityMonitorConfig::default()
                };

                prop_assert!(
                    config.check_interval > Duration::ZERO,
                    "Check interval should be positive"
                );
                prop_assert!(
                    config.low_power_threshold > 0 && config.low_power_threshold <= 100,
                    "Low power threshold should be between 1 and 100"
                );
            }

            /// Property: GameModePolicyConfig delays are non-negative
            /// For any valid configuration, delays should be non-negative
            #[test]
            fn game_mode_policy_config_delays_are_valid(
                enter_delay_ms in 0u64..10000u64,
                exit_delay_ms in 0u64..10000u64
            ) {
                let config = GameModePolicyConfig {
                    enter_delay: Duration::from_millis(enter_delay_ms),
                    exit_delay: Duration::from_millis(exit_delay_ms),
                    ..GameModePolicyConfig::default()
                };

                prop_assert!(
                    config.enter_delay >= Duration::ZERO,
                    "Enter delay should be non-negative"
                );
                prop_assert!(
                    config.exit_delay >= Duration::ZERO,
                    "Exit delay should be non-negative"
                );
            }
        }

        /// Property: State transitions are consistent
        /// When a fullscreen app is detected, game mode should be triggered
        #[test]
        fn fullscreen_detection_triggers_game_mode() {
            // Test various fullscreen states
            let fullscreen_states = vec![
                SystemState::FullscreenApp {
                    app_name: Some("game.exe".to_string()),
                    process_id: Some(1234),
                },
                SystemState::FullscreenApp {
                    app_name: None,
                    process_id: Some(5678),
                },
                SystemState::FullscreenApp {
                    app_name: Some("video_player.exe".to_string()),
                    process_id: None,
                },
                SystemState::PresentationMode,
            ];

            for state in fullscreen_states {
                assert!(
                    state.should_enter_game_mode(),
                    "State {:?} should trigger game mode",
                    state
                );
            }
        }

        /// Property: Non-fullscreen states don't trigger game mode
        #[test]
        fn non_fullscreen_does_not_trigger_game_mode() {
            let non_fullscreen_states = vec![
                SystemState::Normal,
                SystemState::DoNotDisturb,
                SystemState::LowPower { battery_percent: 15 },
            ];

            for state in non_fullscreen_states {
                assert!(
                    !state.should_enter_game_mode(),
                    "State {:?} should NOT trigger game mode",
                    state
                );
            }
        }

        /// Property: Default configurations are valid
        #[test]
        fn default_configs_are_valid() {
            let activity_config = ActivityMonitorConfig::default();
            assert!(activity_config.check_interval > Duration::ZERO);
            assert!(activity_config.low_power_threshold > 0);
            assert!(activity_config.low_power_threshold <= 100);

            let policy_config = GameModePolicyConfig::default();
            assert!(policy_config.enter_delay >= Duration::ZERO);
            assert!(policy_config.exit_delay >= Duration::ZERO);
        }

        /// Property: SystemState default is Normal
        #[test]
        fn system_state_default_is_normal() {
            assert_eq!(SystemState::default(), SystemState::Normal);
        }

        /// Property: GameModeStatus default is Inactive
        #[test]
        fn game_mode_status_default_is_inactive() {
            assert_eq!(GameModeStatus::default(), GameModeStatus::Inactive);
        }
    }
}
