//! Database module for NeuralFS
//!
//! This module provides SQLite database connectivity with WAL mode support
//! for high concurrency operations.

pub mod migration;

#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;

use crate::core::error::{NeuralFSError, Result};

/// SQLite synchronous mode configuration
#[derive(Debug, Clone, Copy, Default)]
pub enum SynchronousMode {
    /// Fastest, but may lose data on crash
    Off,
    /// Balanced performance and safety
    #[default]
    Normal,
    /// Safest, but slowest
    Full,
}

/// SQLite connection pool configuration
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// Database file path
    pub db_path: PathBuf,

    /// Maximum number of connections
    pub max_connections: u32,

    /// Minimum number of connections
    pub min_connections: u32,

    /// Connection timeout in seconds
    pub connect_timeout_secs: u64,

    /// Idle timeout in seconds
    pub idle_timeout_secs: u64,

    /// Whether to enable WAL mode
    pub enable_wal: bool,

    /// Synchronous mode
    pub synchronous: SynchronousMode,

    /// Cache size (pages, negative means KB)
    pub cache_size: i32,

    /// Busy timeout in milliseconds
    pub busy_timeout_ms: u32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            db_path: directories::ProjectDirs::from("com", "neuralfs", "NeuralFS")
                .map(|dirs| dirs.data_local_dir().join("metadata.db"))
                .unwrap_or_else(|| PathBuf::from("metadata.db")),
            max_connections: 10,
            min_connections: 2,
            connect_timeout_secs: 30,
            idle_timeout_secs: 600,
            enable_wal: cfg!(feature = "wal"), // Dynamic based on Cargo feature
            synchronous: SynchronousMode::Normal,
            cache_size: -64000, // 64MB cache
            busy_timeout_ms: 5000, // 5 second busy timeout
        }
    }
}

impl DatabaseConfig {
    /// Create a new DatabaseConfig with the specified path
    pub fn with_path(db_path: PathBuf) -> Self {
        Self {
            db_path,
            ..Default::default()
        }
    }

    /// Set WAL mode
    pub fn with_wal(mut self, enable: bool) -> Self {
        self.enable_wal = enable;
        self
    }

    /// Set maximum connections
    pub fn with_max_connections(mut self, max: u32) -> Self {
        self.max_connections = max;
        self
    }

    /// Set synchronous mode
    pub fn with_synchronous(mut self, mode: SynchronousMode) -> Self {
        self.synchronous = mode;
        self
    }
}

/// Create a database connection pool with the given configuration
///
/// This function creates a SQLite connection pool with WAL mode support
/// for high concurrency operations. The WAL mode is configured based on
/// the `wal` feature flag in Cargo.toml.
///
/// # Arguments
///
/// * `config` - Database configuration
///
/// # Returns
///
/// A SQLite connection pool
///
/// # Errors
///
/// Returns an error if the database cannot be created or connected to
pub async fn create_database_pool(config: &DatabaseConfig) -> Result<SqlitePool> {
    // Ensure parent directory exists
    if let Some(parent) = config.db_path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| {
            NeuralFSError::Database(sqlx::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to create database directory: {}", e),
            )))
        })?;
    }

    // Build connection options
    let connect_options = SqliteConnectOptions::new()
        .filename(&config.db_path)
        .create_if_missing(true)
        // WAL mode: allows concurrent reads and writes
        // Dynamically configured based on Cargo.toml `wal` feature
        .journal_mode(if config.enable_wal {
            SqliteJournalMode::Wal
        } else {
            SqliteJournalMode::Delete
        })
        // Synchronous mode
        .synchronous(match config.synchronous {
            SynchronousMode::Off => SqliteSynchronous::Off,
            SynchronousMode::Normal => SqliteSynchronous::Normal,
            SynchronousMode::Full => SqliteSynchronous::Full,
        })
        // Busy timeout
        .busy_timeout(Duration::from_millis(config.busy_timeout_ms as u64))
        // Foreign key constraints
        .foreign_keys(true);

    // Create connection pool
    let pool = SqlitePoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(Duration::from_secs(config.connect_timeout_secs))
        .idle_timeout(Duration::from_secs(config.idle_timeout_secs))
        .connect_with(connect_options)
        .await
        .map_err(NeuralFSError::Database)?;

    // Set PRAGMA options for each connection
    sqlx::query(&format!("PRAGMA cache_size = {}", config.cache_size))
        .execute(&pool)
        .await
        .map_err(NeuralFSError::Database)?;

    // Enable memory-mapped I/O (improves read performance)
    sqlx::query("PRAGMA mmap_size = 268435456") // 256MB
        .execute(&pool)
        .await
        .map_err(NeuralFSError::Database)?;

    // Optimize temporary storage
    sqlx::query("PRAGMA temp_store = MEMORY")
        .execute(&pool)
        .await
        .map_err(NeuralFSError::Database)?;

    tracing::info!(
        "Database pool created: {:?} (WAL: {}, connections: {})",
        config.db_path,
        config.enable_wal,
        config.max_connections
    );

    Ok(pool)
}

/// WAL checkpoint manager for periodic checkpointing
pub struct WalCheckpointManager {
    pool: SqlitePool,
    checkpoint_interval: Duration,
}

impl WalCheckpointManager {
    /// Create a new WAL checkpoint manager
    pub fn new(pool: SqlitePool, checkpoint_interval: Duration) -> Self {
        Self {
            pool,
            checkpoint_interval,
        }
    }

    /// Start periodic checkpointing in a background task
    pub fn start(&self) {
        let pool = self.pool.clone();
        let interval = self.checkpoint_interval;

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;

                // Execute passive checkpoint (doesn't block writes)
                if let Err(e) = sqlx::query("PRAGMA wal_checkpoint(PASSIVE)")
                    .execute(&pool)
                    .await
                {
                    tracing::warn!("WAL checkpoint failed: {}", e);
                }
            }
        });
    }

    /// Execute a full checkpoint (should be called on application exit)
    pub async fn full_checkpoint(&self) -> Result<()> {
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&self.pool)
            .await
            .map_err(NeuralFSError::Database)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_create_database_pool() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let config = DatabaseConfig::with_path(db_path.clone());
        let pool = create_database_pool(&config).await.unwrap();

        // Verify pool is working
        let result: (i32,) = sqlx::query_as("SELECT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(result.0, 1);

        pool.close().await;
    }

    #[tokio::test]
    async fn test_wal_mode_enabled() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_wal.db");

        let config = DatabaseConfig::with_path(db_path).with_wal(true);
        let pool = create_database_pool(&config).await.unwrap();

        // Check journal mode
        let result: (String,) = sqlx::query_as("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(result.0.to_lowercase(), "wal");

        pool.close().await;
    }

    #[tokio::test]
    async fn test_delete_mode_when_wal_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_delete.db");

        let config = DatabaseConfig::with_path(db_path).with_wal(false);
        let pool = create_database_pool(&config).await.unwrap();

        // Check journal mode
        let result: (String,) = sqlx::query_as("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(result.0.to_lowercase(), "delete");

        pool.close().await;
    }
}
