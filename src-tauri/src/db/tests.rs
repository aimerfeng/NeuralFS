//! Property-based tests for database module
//!
//! These tests verify correctness properties for the database layer.

#[cfg(test)]
mod property_tests {
    use proptest::prelude::*;
    use tempfile::TempDir;
    use tokio::runtime::Runtime;

    use crate::db::{create_database_pool, DatabaseConfig};
    use crate::db::migration::{Migration, MigrationManager};

    /// Helper to create a test database
    fn setup_test_db() -> (sqlx::SqlitePool, TempDir, Runtime) {
        let rt = Runtime::new().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let config = DatabaseConfig::with_path(db_path).with_wal(true);
        let pool = rt.block_on(create_database_pool(&config)).unwrap();
        (pool, temp_dir, rt)
    }

    /// Generate a valid table name (alphanumeric, starts with letter)
    fn table_name_strategy() -> impl Strategy<Value = String> {
        "[a-z][a-z0-9_]{2,15}".prop_map(|s| s.to_string())
    }

    /// Generate a valid column definition
    fn column_def_strategy() -> impl Strategy<Value = String> {
        (
            "[a-z][a-z0-9_]{1,10}",
            prop_oneof![
                Just("INTEGER"),
                Just("TEXT"),
                Just("REAL"),
                Just("BLOB"),
            ],
        )
            .prop_map(|(name, typ)| format!("{} {}", name, typ))
    }

    /// Generate a valid CREATE TABLE statement
    fn create_table_strategy() -> impl Strategy<Value = (String, String)> {
        (
            table_name_strategy(),
            prop::collection::vec(column_def_strategy(), 1..5),
        )
            .prop_map(|(table_name, columns)| {
                let cols = columns.join(", ");
                let sql = format!("CREATE TABLE {} (id INTEGER PRIMARY KEY, {})", table_name, cols);
                (table_name, sql)
            })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Feature: neural-fs-core, Property 32: Migration Atomicity**
        /// *For any* database migration, either all SQL statements in the migration
        /// succeed and the version is recorded, or the database is rolled back to
        /// the pre-migration state.
        /// **Validates: Schema Migration, Requirements 18**
        #[test]
        fn prop_migration_atomicity(
            (table_name, create_sql) in create_table_strategy(),
            version in 1i64..1000,
        ) {
            let (pool, _temp_dir, rt) = setup_test_db();

            rt.block_on(async {
                let mut manager = MigrationManager::new(pool.clone());

                // Create a valid migration
                let migration = Migration::new(version, &table_name, &create_sql);
                manager.add_migration(migration);

                // Get initial state
                let initial_version = manager.current_version().await.unwrap();

                // Apply migration
                let result = manager.migrate().await;

                match result {
                    Ok(migration_result) => {
                        // Success case: version should be updated
                        let new_version = manager.current_version().await.unwrap();
                        prop_assert!(
                            new_version >= initial_version,
                            "Version should not decrease after successful migration"
                        );

                        // Table should exist
                        let table_exists: Option<(String,)> = sqlx::query_as(
                            &format!(
                                "SELECT name FROM sqlite_master WHERE type='table' AND name='{}'",
                                table_name
                            )
                        )
                        .fetch_optional(&pool)
                        .await
                        .unwrap();

                        prop_assert!(
                            table_exists.is_some(),
                            "Table should exist after successful migration"
                        );

                        // Migration should be recorded
                        let applied = manager.applied_migrations().await.unwrap();
                        prop_assert!(
                            applied.contains_key(&version),
                            "Migration should be recorded in schema_migrations"
                        );
                    }
                    Err(_) => {
                        // Failure case: version should remain unchanged
                        let new_version = manager.current_version().await.unwrap();
                        prop_assert_eq!(
                            new_version,
                            initial_version,
                            "Version should remain unchanged after failed migration"
                        );

                        // Table should NOT exist (rolled back)
                        let table_exists: Option<(String,)> = sqlx::query_as(
                            &format!(
                                "SELECT name FROM sqlite_master WHERE type='table' AND name='{}'",
                                table_name
                            )
                        )
                        .fetch_optional(&pool)
                        .await
                        .unwrap();

                        prop_assert!(
                            table_exists.is_none(),
                            "Table should not exist after failed migration (atomicity)"
                        );
                    }
                }

                Ok(())
            })?;
        }

        /// **Feature: neural-fs-core, Property 32: Migration Atomicity (Multi-statement)**
        /// *For any* migration with multiple statements, if any statement fails,
        /// all previous statements should be rolled back.
        /// **Validates: Schema Migration, Requirements 18**
        #[test]
        fn prop_migration_atomicity_multi_statement(
            (table1, create1) in create_table_strategy(),
            (table2, create2) in create_table_strategy(),
            fail_second in prop::bool::ANY,
        ) {
            // Skip if table names are the same
            prop_assume!(table1 != table2);

            let (pool, _temp_dir, rt) = setup_test_db();

            rt.block_on(async {
                let mut manager = MigrationManager::new(pool.clone());

                // Create migration with two statements
                let sql = if fail_second {
                    // Second statement will fail (invalid SQL)
                    format!("{}; INVALID SQL SYNTAX HERE", create1)
                } else {
                    // Both statements valid
                    format!("{}; {}", create1, create2)
                };

                let migration = Migration::new(1, "multi_statement", &sql);
                manager.add_migration(migration);

                let result = manager.migrate().await;

                if fail_second {
                    // Should fail and rollback
                    prop_assert!(result.is_err(), "Migration with invalid SQL should fail");

                    // First table should NOT exist (rolled back)
                    let table1_exists: Option<(String,)> = sqlx::query_as(
                        &format!(
                            "SELECT name FROM sqlite_master WHERE type='table' AND name='{}'",
                            table1
                        )
                    )
                    .fetch_optional(&pool)
                    .await
                    .unwrap();

                    prop_assert!(
                        table1_exists.is_none(),
                        "First table should be rolled back when second statement fails"
                    );

                    // Version should be 0
                    let version = manager.current_version().await.unwrap();
                    prop_assert_eq!(version, 0, "Version should be 0 after failed migration");
                } else {
                    // Should succeed
                    prop_assert!(result.is_ok(), "Migration with valid SQL should succeed");

                    // Both tables should exist
                    let table1_exists: Option<(String,)> = sqlx::query_as(
                        &format!(
                            "SELECT name FROM sqlite_master WHERE type='table' AND name='{}'",
                            table1
                        )
                    )
                    .fetch_optional(&pool)
                    .await
                    .unwrap();

                    let table2_exists: Option<(String,)> = sqlx::query_as(
                        &format!(
                            "SELECT name FROM sqlite_master WHERE type='table' AND name='{}'",
                            table2
                        )
                    )
                    .fetch_optional(&pool)
                    .await
                    .unwrap();

                    prop_assert!(table1_exists.is_some(), "First table should exist");
                    prop_assert!(table2_exists.is_some(), "Second table should exist");
                }

                Ok(())
            })?;
        }
    }
}


#[cfg(test)]
mod wal_property_tests {
    use proptest::prelude::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::TempDir;
    use tokio::runtime::Runtime;
    use tokio::sync::Barrier;

    use crate::db::{create_database_pool, DatabaseConfig};

    /// Helper to create a test database with WAL mode
    fn setup_wal_db() -> (sqlx::SqlitePool, TempDir, Runtime) {
        let rt = Runtime::new().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_wal.db");
        let config = DatabaseConfig::with_path(db_path)
            .with_wal(true)
            .with_max_connections(10);
        let pool = rt.block_on(create_database_pool(&config)).unwrap();
        
        // Create test table
        rt.block_on(async {
            sqlx::query("CREATE TABLE IF NOT EXISTS wal_test (id INTEGER PRIMARY KEY, value INTEGER)")
                .execute(&pool)
                .await
                .unwrap();
        });
        
        (pool, temp_dir, rt)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Feature: neural-fs-core, Property 35: WAL Mode Concurrency**
        /// *For any* concurrent read and write operations on the SQLite database,
        /// the WAL mode SHALL allow reads to proceed without blocking on writes.
        /// **Validates: SQLite High Concurrency**
        #[test]
        fn prop_wal_concurrent_read_write(
            num_readers in 2usize..5,
            num_writers in 1usize..3,
            iterations in 5usize..20,
        ) {
            let (pool, _temp_dir, rt) = setup_wal_db();

            rt.block_on(async {
                let pool = Arc::new(pool);
                let barrier = Arc::new(Barrier::new(num_readers + num_writers));
                let successful_reads = Arc::new(AtomicUsize::new(0));
                let successful_writes = Arc::new(AtomicUsize::new(0));
                let blocked_reads = Arc::new(AtomicUsize::new(0));

                let mut handles = Vec::new();

                // Spawn writer tasks
                for writer_id in 0..num_writers {
                    let pool = Arc::clone(&pool);
                    let barrier = Arc::clone(&barrier);
                    let successful_writes = Arc::clone(&successful_writes);

                    let handle = tokio::spawn(async move {
                        barrier.wait().await;

                        for i in 0..iterations {
                            let value = (writer_id * 1000 + i) as i64;
                            let result = sqlx::query("INSERT INTO wal_test (value) VALUES (?)")
                                .bind(value)
                                .execute(&*pool)
                                .await;

                            if result.is_ok() {
                                successful_writes.fetch_add(1, Ordering::SeqCst);
                            }

                            // Small delay to simulate real workload
                            tokio::time::sleep(tokio::time::Duration::from_micros(100)).await;
                        }
                    });
                    handles.push(handle);
                }

                // Spawn reader tasks
                for _reader_id in 0..num_readers {
                    let pool = Arc::clone(&pool);
                    let barrier = Arc::clone(&barrier);
                    let successful_reads = Arc::clone(&successful_reads);
                    let blocked_reads = Arc::clone(&blocked_reads);

                    let handle = tokio::spawn(async move {
                        barrier.wait().await;

                        for _ in 0..iterations {
                            let start = std::time::Instant::now();
                            
                            let result: Result<Vec<(i64,)>, _> = sqlx::query_as(
                                "SELECT value FROM wal_test LIMIT 100"
                            )
                            .fetch_all(&*pool)
                            .await;

                            let elapsed = start.elapsed();

                            if result.is_ok() {
                                successful_reads.fetch_add(1, Ordering::SeqCst);
                                
                                // If read took more than 100ms, consider it blocked
                                // (WAL should allow reads to proceed quickly)
                                if elapsed.as_millis() > 100 {
                                    blocked_reads.fetch_add(1, Ordering::SeqCst);
                                }
                            }

                            // Small delay to simulate real workload
                            tokio::time::sleep(tokio::time::Duration::from_micros(50)).await;
                        }
                    });
                    handles.push(handle);
                }

                // Wait for all tasks to complete
                for handle in handles {
                    handle.await.unwrap();
                }

                let total_reads = successful_reads.load(Ordering::SeqCst);
                let total_writes = successful_writes.load(Ordering::SeqCst);
                let blocked = blocked_reads.load(Ordering::SeqCst);

                // Property assertions
                
                // 1. All reads should succeed (WAL allows concurrent reads)
                let expected_reads = num_readers * iterations;
                prop_assert!(
                    total_reads >= expected_reads * 90 / 100,
                    "At least 90% of reads should succeed. Got {}/{}", 
                    total_reads, expected_reads
                );

                // 2. All writes should succeed
                let expected_writes = num_writers * iterations;
                prop_assert!(
                    total_writes >= expected_writes * 90 / 100,
                    "At least 90% of writes should succeed. Got {}/{}", 
                    total_writes, expected_writes
                );

                // 3. Reads should not be blocked by writes (WAL property)
                // Allow up to 10% blocked reads due to system variance
                let blocked_ratio = if total_reads > 0 {
                    blocked as f64 / total_reads as f64
                } else {
                    0.0
                };
                prop_assert!(
                    blocked_ratio < 0.1,
                    "Less than 10% of reads should be blocked. Got {:.1}% ({}/{})",
                    blocked_ratio * 100.0, blocked, total_reads
                );

                Ok(())
            })?;
        }

        /// **Feature: neural-fs-core, Property 35: WAL Mode Concurrency (Read Isolation)**
        /// *For any* read operation during concurrent writes, the read should see
        /// a consistent snapshot of the database.
        /// **Validates: SQLite High Concurrency**
        #[test]
        fn prop_wal_read_isolation(
            num_inserts in 10usize..50,
        ) {
            let (pool, _temp_dir, rt) = setup_wal_db();

            rt.block_on(async {
                let pool = Arc::new(pool);

                // Insert initial data
                for i in 0..num_inserts {
                    sqlx::query("INSERT INTO wal_test (value) VALUES (?)")
                        .bind(i as i64)
                        .execute(&*pool)
                        .await
                        .unwrap();
                }

                // Start a read transaction
                let read_pool = Arc::clone(&pool);
                let read_handle = tokio::spawn(async move {
                    // Read count multiple times during writes
                    let mut counts = Vec::new();
                    for _ in 0..5 {
                        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM wal_test")
                            .fetch_one(&*read_pool)
                            .await
                            .unwrap();
                        counts.push(count.0);
                        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                    }
                    counts
                });

                // Concurrent writes
                let write_pool = Arc::clone(&pool);
                let write_handle = tokio::spawn(async move {
                    for i in 0..10 {
                        sqlx::query("INSERT INTO wal_test (value) VALUES (?)")
                            .bind((num_inserts + i) as i64)
                            .execute(&*write_pool)
                            .await
                            .unwrap();
                        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
                    }
                });

                let counts = read_handle.await.unwrap();
                write_handle.await.unwrap();

                // Property: Each read should see a consistent count
                // (counts should be monotonically non-decreasing)
                for i in 1..counts.len() {
                    prop_assert!(
                        counts[i] >= counts[i - 1],
                        "Read counts should be monotonically non-decreasing: {:?}",
                        counts
                    );
                }

                // Property: All counts should be valid (>= initial inserts)
                for count in &counts {
                    prop_assert!(
                        *count >= num_inserts as i64,
                        "Count should be at least initial inserts: {} >= {}",
                        count, num_inserts
                    );
                }

                Ok(())
            })?;
        }
    }
}
