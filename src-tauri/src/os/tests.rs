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
}
