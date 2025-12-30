//! Database migration manager for NeuralFS
//!
//! Provides automatic schema migration with atomic transactions and rollback support.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::Utc;
use sqlx::SqlitePool;

use crate::core::error::{DatabaseError, NeuralFSError, Result};

/// Represents a single database migration
#[derive(Debug, Clone)]
pub struct Migration {
    /// Migration version number
    pub version: i64,
    /// Migration name
    pub name: String,
    /// SQL statements to apply the migration
    pub up_sql: String,
    /// SQL statements to rollback the migration (optional)
    pub down_sql: Option<String>,
    /// Checksum for integrity verification
    pub checksum: String,
}

impl Migration {
    /// Create a new migration
    pub fn new(version: i64, name: impl Into<String>, up_sql: impl Into<String>) -> Self {
        let up_sql = up_sql.into();
        let checksum = Self::calculate_checksum(&up_sql);
        Self {
            version,
            name: name.into(),
            up_sql,
            down_sql: None,
            checksum,
        }
    }

    /// Add rollback SQL
    pub fn with_down(mut self, down_sql: impl Into<String>) -> Self {
        self.down_sql = Some(down_sql.into());
        self
    }

    /// Calculate checksum for migration content
    fn calculate_checksum(content: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}

/// Migration manager for handling database schema updates
pub struct MigrationManager {
    pool: SqlitePool,
    migrations: Vec<Migration>,
}

impl MigrationManager {
    /// Create a new migration manager
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            migrations: Vec::new(),
        }
    }

    /// Add a migration to the manager
    pub fn add_migration(&mut self, migration: Migration) {
        self.migrations.push(migration);
        // Keep migrations sorted by version
        self.migrations.sort_by_key(|m| m.version);
    }

    /// Load migrations from embedded SQL files
    pub fn with_embedded_migrations(mut self) -> Self {
        // Add the initial schema migration
        let initial_migration = Migration::new(
            1,
            "001_initial_schema",
            include_str!("../../migrations/001_initial_schema.sql"),
        );
        self.add_migration(initial_migration);
        self
    }

    /// Load migrations from a directory
    pub async fn load_migrations_from_dir(&mut self, dir: &Path) -> Result<()> {
        let mut entries = tokio::fs::read_dir(dir).await.map_err(|e| {
            NeuralFSError::DatabaseInternal(DatabaseError::MigrationFailed {
                reason: format!("Failed to read migrations directory: {}", e),
            })
        })?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            NeuralFSError::DatabaseInternal(DatabaseError::MigrationFailed {
                reason: format!("Failed to read migration entry: {}", e),
            })
        })? {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "sql") {
                if let Some(migration) = self.parse_migration_file(&path).await? {
                    self.add_migration(migration);
                }
            }
        }

        Ok(())
    }

    /// Parse a migration file
    async fn parse_migration_file(&self, path: &Path) -> Result<Option<Migration>> {
        let filename = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| {
                NeuralFSError::DatabaseInternal(DatabaseError::MigrationFailed {
                    reason: "Invalid migration filename".to_string(),
                })
            })?;

        // Parse version from filename (e.g., "001_initial_schema" -> 1)
        let version: i64 = filename
            .split('_')
            .next()
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| {
                NeuralFSError::DatabaseInternal(DatabaseError::MigrationFailed {
                    reason: format!("Invalid migration version in filename: {}", filename),
                })
            })?;

        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
            NeuralFSError::DatabaseInternal(DatabaseError::MigrationFailed {
                reason: format!("Failed to read migration file: {}", e),
            })
        })?;

        Ok(Some(Migration::new(version, filename, content)))
    }

    /// Ensure the migrations table exists
    async fn ensure_migrations_table(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                applied_at TEXT NOT NULL,
                checksum TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(NeuralFSError::Database)?;

        Ok(())
    }

    /// Get the current schema version
    pub async fn current_version(&self) -> Result<i64> {
        self.ensure_migrations_table().await?;

        let result: Option<(i64,)> = sqlx::query_as(
            "SELECT MAX(version) FROM schema_migrations"
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(NeuralFSError::Database)?;

        Ok(result.and_then(|r| Some(r.0)).unwrap_or(0))
    }

    /// Get list of applied migrations
    pub async fn applied_migrations(&self) -> Result<HashMap<i64, AppliedMigration>> {
        self.ensure_migrations_table().await?;

        let rows: Vec<(i64, String, String, String)> = sqlx::query_as(
            "SELECT version, name, applied_at, checksum FROM schema_migrations ORDER BY version"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(NeuralFSError::Database)?;

        let mut applied = HashMap::new();
        for (version, name, applied_at, checksum) in rows {
            applied.insert(version, AppliedMigration {
                version,
                name,
                applied_at,
                checksum,
            });
        }

        Ok(applied)
    }

    /// Run all pending migrations
    ///
    /// This method applies all migrations that haven't been applied yet,
    /// in order of their version numbers. Each migration is run in a
    /// transaction to ensure atomicity.
    pub async fn migrate(&self) -> Result<MigrationResult> {
        self.ensure_migrations_table().await?;

        let applied = self.applied_migrations().await?;
        let mut result = MigrationResult::default();

        for migration in &self.migrations {
            if applied.contains_key(&migration.version) {
                // Verify checksum
                let applied_migration = &applied[&migration.version];
                if applied_migration.checksum != migration.checksum {
                    return Err(NeuralFSError::DatabaseInternal(DatabaseError::MigrationFailed {
                        reason: format!(
                            "Migration {} checksum mismatch: expected {}, found {}",
                            migration.version, migration.checksum, applied_migration.checksum
                        ),
                    }));
                }
                result.skipped += 1;
                continue;
            }

            // Apply migration in a transaction
            self.apply_migration(migration).await?;
            result.applied += 1;
            result.applied_versions.push(migration.version);
        }

        result.current_version = self.current_version().await?;
        Ok(result)
    }

    /// Apply a single migration atomically
    ///
    /// The migration is wrapped in a transaction. If any statement fails,
    /// the entire migration is rolled back.
    async fn apply_migration(&self, migration: &Migration) -> Result<()> {
        tracing::info!(
            "Applying migration {}: {}",
            migration.version,
            migration.name
        );

        // Start a transaction
        let mut tx = self.pool.begin().await.map_err(NeuralFSError::Database)?;

        // Execute migration SQL
        // Split by semicolons and execute each statement
        for statement in migration.up_sql.split(';') {
            let statement = statement.trim();
            if statement.is_empty() || statement.starts_with("--") {
                continue;
            }

            sqlx::query(statement)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    NeuralFSError::DatabaseInternal(DatabaseError::MigrationFailed {
                        reason: format!(
                            "Migration {} failed at statement: {}. Error: {}",
                            migration.version, statement, e
                        ),
                    })
                })?;
        }

        // Record the migration
        let applied_at = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO schema_migrations (version, name, applied_at, checksum)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(migration.version)
        .bind(&migration.name)
        .bind(&applied_at)
        .bind(&migration.checksum)
        .execute(&mut *tx)
        .await
        .map_err(NeuralFSError::Database)?;

        // Commit the transaction
        tx.commit().await.map_err(NeuralFSError::Database)?;

        tracing::info!(
            "Migration {} applied successfully",
            migration.version
        );

        Ok(())
    }

    /// Rollback the last applied migration
    ///
    /// This only works if the migration has a down_sql defined.
    pub async fn rollback(&self) -> Result<Option<i64>> {
        let current = self.current_version().await?;
        if current == 0 {
            return Ok(None);
        }

        // Find the migration to rollback
        let migration = self
            .migrations
            .iter()
            .find(|m| m.version == current)
            .ok_or_else(|| {
                NeuralFSError::DatabaseInternal(DatabaseError::MigrationFailed {
                    reason: format!("Migration {} not found for rollback", current),
                })
            })?;

        let down_sql = migration.down_sql.as_ref().ok_or_else(|| {
            NeuralFSError::DatabaseInternal(DatabaseError::MigrationFailed {
                reason: format!("Migration {} has no rollback SQL", current),
            })
        })?;

        tracing::info!("Rolling back migration {}: {}", migration.version, migration.name);

        // Start a transaction
        let mut tx = self.pool.begin().await.map_err(NeuralFSError::Database)?;

        // Execute rollback SQL
        for statement in down_sql.split(';') {
            let statement = statement.trim();
            if statement.is_empty() || statement.starts_with("--") {
                continue;
            }

            sqlx::query(statement)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    NeuralFSError::DatabaseInternal(DatabaseError::MigrationFailed {
                        reason: format!(
                            "Rollback {} failed at statement: {}. Error: {}",
                            migration.version, statement, e
                        ),
                    })
                })?;
        }

        // Remove the migration record
        sqlx::query("DELETE FROM schema_migrations WHERE version = ?")
            .bind(migration.version)
            .execute(&mut *tx)
            .await
            .map_err(NeuralFSError::Database)?;

        // Commit the transaction
        tx.commit().await.map_err(NeuralFSError::Database)?;

        tracing::info!("Migration {} rolled back successfully", migration.version);

        Ok(Some(current))
    }

    /// Rollback all migrations to a specific version
    pub async fn rollback_to(&self, target_version: i64) -> Result<Vec<i64>> {
        let mut rolled_back = Vec::new();

        loop {
            let current = self.current_version().await?;
            if current <= target_version {
                break;
            }

            if let Some(version) = self.rollback().await? {
                rolled_back.push(version);
            } else {
                break;
            }
        }

        Ok(rolled_back)
    }
}

/// Information about an applied migration
#[derive(Debug, Clone)]
pub struct AppliedMigration {
    pub version: i64,
    pub name: String,
    pub applied_at: String,
    pub checksum: String,
}

/// Result of running migrations
#[derive(Debug, Default)]
pub struct MigrationResult {
    /// Number of migrations applied
    pub applied: usize,
    /// Number of migrations skipped (already applied)
    pub skipped: usize,
    /// Versions that were applied
    pub applied_versions: Vec<i64>,
    /// Current schema version after migration
    pub current_version: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::db::{create_database_pool, DatabaseConfig};

    async fn setup_test_db() -> (SqlitePool, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_migration.db");
        let config = DatabaseConfig::with_path(db_path).with_wal(true);
        let pool = create_database_pool(&config).await.unwrap();
        (pool, temp_dir)
    }

    #[tokio::test]
    async fn test_migration_manager_creation() {
        let (pool, _temp_dir) = setup_test_db().await;
        let manager = MigrationManager::new(pool);
        assert!(manager.migrations.is_empty());
    }

    #[tokio::test]
    async fn test_add_migration() {
        let (pool, _temp_dir) = setup_test_db().await;
        let mut manager = MigrationManager::new(pool);
        
        let migration = Migration::new(1, "test_migration", "CREATE TABLE test (id INTEGER)");
        manager.add_migration(migration);
        
        assert_eq!(manager.migrations.len(), 1);
        assert_eq!(manager.migrations[0].version, 1);
    }

    #[tokio::test]
    async fn test_migrations_sorted_by_version() {
        let (pool, _temp_dir) = setup_test_db().await;
        let mut manager = MigrationManager::new(pool);
        
        // Add migrations out of order
        manager.add_migration(Migration::new(3, "third", "SELECT 3"));
        manager.add_migration(Migration::new(1, "first", "SELECT 1"));
        manager.add_migration(Migration::new(2, "second", "SELECT 2"));
        
        assert_eq!(manager.migrations[0].version, 1);
        assert_eq!(manager.migrations[1].version, 2);
        assert_eq!(manager.migrations[2].version, 3);
    }

    #[tokio::test]
    async fn test_current_version_empty_db() {
        let (pool, _temp_dir) = setup_test_db().await;
        let manager = MigrationManager::new(pool);
        
        let version = manager.current_version().await.unwrap();
        assert_eq!(version, 0);
    }

    #[tokio::test]
    async fn test_apply_single_migration() {
        let (pool, _temp_dir) = setup_test_db().await;
        let mut manager = MigrationManager::new(pool.clone());
        
        let migration = Migration::new(
            1,
            "create_test_table",
            "CREATE TABLE test_table (id INTEGER PRIMARY KEY, name TEXT)",
        );
        manager.add_migration(migration);
        
        let result = manager.migrate().await.unwrap();
        assert_eq!(result.applied, 1);
        assert_eq!(result.skipped, 0);
        assert_eq!(result.current_version, 1);
        
        // Verify table exists
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM test_table")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 0);
    }

    #[tokio::test]
    async fn test_skip_already_applied_migration() {
        let (pool, _temp_dir) = setup_test_db().await;
        let mut manager = MigrationManager::new(pool);
        
        let migration = Migration::new(
            1,
            "create_test_table",
            "CREATE TABLE test_table (id INTEGER PRIMARY KEY)",
        );
        manager.add_migration(migration);
        
        // Apply first time
        let result1 = manager.migrate().await.unwrap();
        assert_eq!(result1.applied, 1);
        
        // Apply second time - should skip
        let result2 = manager.migrate().await.unwrap();
        assert_eq!(result2.applied, 0);
        assert_eq!(result2.skipped, 1);
    }

    #[tokio::test]
    async fn test_migration_atomicity_on_failure() {
        let (pool, _temp_dir) = setup_test_db().await;
        let mut manager = MigrationManager::new(pool.clone());
        
        // Migration with invalid SQL that will fail
        let migration = Migration::new(
            1,
            "failing_migration",
            r#"
            CREATE TABLE valid_table (id INTEGER);
            INVALID SQL STATEMENT;
            "#,
        );
        manager.add_migration(migration);
        
        // Migration should fail
        let result = manager.migrate().await;
        assert!(result.is_err());
        
        // Verify the valid_table was NOT created (rolled back)
        let table_exists: Option<(String,)> = sqlx::query_as(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='valid_table'"
        )
        .fetch_optional(&pool)
        .await
        .unwrap();
        
        assert!(table_exists.is_none(), "Table should not exist after failed migration");
        
        // Verify version is still 0
        let version = manager.current_version().await.unwrap();
        assert_eq!(version, 0);
    }

    #[tokio::test]
    async fn test_rollback_migration() {
        let (pool, _temp_dir) = setup_test_db().await;
        let mut manager = MigrationManager::new(pool.clone());
        
        let migration = Migration::new(
            1,
            "create_test_table",
            "CREATE TABLE test_table (id INTEGER PRIMARY KEY)",
        )
        .with_down("DROP TABLE test_table");
        manager.add_migration(migration);
        
        // Apply migration
        manager.migrate().await.unwrap();
        assert_eq!(manager.current_version().await.unwrap(), 1);
        
        // Rollback
        let rolled_back = manager.rollback().await.unwrap();
        assert_eq!(rolled_back, Some(1));
        assert_eq!(manager.current_version().await.unwrap(), 0);
        
        // Verify table no longer exists
        let table_exists: Option<(String,)> = sqlx::query_as(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='test_table'"
        )
        .fetch_optional(&pool)
        .await
        .unwrap();
        
        assert!(table_exists.is_none());
    }

    #[tokio::test]
    async fn test_embedded_migrations() {
        let (pool, _temp_dir) = setup_test_db().await;
        let manager = MigrationManager::new(pool.clone()).with_embedded_migrations();
        
        // Should have at least the initial migration
        assert!(!manager.migrations.is_empty());
        assert_eq!(manager.migrations[0].version, 1);
        assert_eq!(manager.migrations[0].name, "001_initial_schema");
    }

    #[tokio::test]
    async fn test_apply_embedded_migrations() {
        let (pool, _temp_dir) = setup_test_db().await;
        let manager = MigrationManager::new(pool.clone()).with_embedded_migrations();
        
        let result = manager.migrate().await.unwrap();
        assert!(result.applied > 0);
        
        // Verify core tables exist
        let tables: Vec<(String,)> = sqlx::query_as(
            "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        
        let table_names: Vec<&str> = tables.iter().map(|t| t.0.as_str()).collect();
        assert!(table_names.contains(&"files"));
        assert!(table_names.contains(&"content_chunks"));
        assert!(table_names.contains(&"tags"));
        assert!(table_names.contains(&"file_tags"));
        assert!(table_names.contains(&"file_relations"));
    }

    #[tokio::test]
    async fn test_checksum_verification() {
        let (pool, _temp_dir) = setup_test_db().await;
        
        // First manager with original migration
        let mut manager1 = MigrationManager::new(pool.clone());
        manager1.add_migration(Migration::new(1, "test", "CREATE TABLE t1 (id INTEGER)"));
        manager1.migrate().await.unwrap();
        
        // Second manager with modified migration (same version, different content)
        let mut manager2 = MigrationManager::new(pool);
        manager2.add_migration(Migration::new(1, "test", "CREATE TABLE t2 (id INTEGER)"));
        
        // Should fail due to checksum mismatch
        let result = manager2.migrate().await;
        assert!(result.is_err());
        
        if let Err(NeuralFSError::DatabaseInternal(DatabaseError::MigrationFailed { reason })) = result {
            assert!(reason.contains("checksum mismatch"));
        } else {
            panic!("Expected checksum mismatch error");
        }
    }
}
