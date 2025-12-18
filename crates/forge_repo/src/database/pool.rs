//! Database connection pool management with retry logic and preflight checks
//!
//! This module provides robust SQLite connection pooling with:
//! - **Preflight checks**: Validates disk space, permissions, and file
//!   accessibility before connection
//! - **Retry logic**: Exponential backoff retry mechanism using `backon`
//!   library for transient failures
//! - **Actionable errors**: Detailed error messages with troubleshooting steps
//! - **WAL mode**: Configured for better concurrency with Write-Ahead Logging
//!
//! # Error Scenarios Handled
//!
//! The pool creation process handles and provides actionable guidance for:
//! - Insufficient disk space (minimum 100 MB required)
//! - Permission issues (read/write access to database and directory)
//! - Corrupted database files
//! - File locking conflicts
//! - Disk I/O errors
//! - Filesystem compatibility issues (WAL mode requirements)

#![allow(dead_code)]
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use backon::{BlockingRetryable, ExponentialBuilder};
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, CustomizeConnection, Pool, PooledConnection};
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use tracing::{debug, warn};

use super::preflight;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("src/database/migrations");

pub type DbPool = Pool<ConnectionManager<SqliteConnection>>;
pub type PooledSqliteConnection = PooledConnection<ConnectionManager<SqliteConnection>>;

#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub max_size: u32,
    pub min_idle: Option<u32>,
    pub connection_timeout: Duration,
    pub idle_timeout: Option<Duration>,
    pub database_path: PathBuf,
}

impl PoolConfig {
    pub fn new(database_path: PathBuf) -> Self {
        Self {
            max_size: 5,
            min_idle: Some(1),
            connection_timeout: Duration::from_secs(30),
            idle_timeout: Some(Duration::from_secs(600)), // 10 minutes
            database_path,
        }
    }
}

#[derive(Debug)]
pub struct DatabasePool {
    pool: DbPool,
}

impl DatabasePool {
    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        debug!("Creating in-memory database pool");

        let manager = ConnectionManager::<SqliteConnection>::new(":memory:");

        let pool = Pool::builder()
            .max_size(1) // Single connection for in-memory testing
            .connection_timeout(Duration::from_secs(30))
            .build(manager)
            .map_err(|e| anyhow::anyhow!("Failed to create in-memory connection pool: {e}"))?;

        // Run migrations on the in-memory database
        let mut connection = pool
            .get()
            .map_err(|e| anyhow::anyhow!("Failed to get connection for migrations: {e}"))?;

        connection
            .run_pending_migrations(MIGRATIONS)
            .map_err(|e| anyhow::anyhow!("Failed to run database migrations: {e}"))?;

        Ok(Self { pool })
    }

    pub fn get_connection(&self) -> Result<PooledSqliteConnection> {
        self.pool.get().map_err(|e| {
            warn!(error = %e, "Failed to get connection from pool");
            anyhow::anyhow!("Failed to get connection from pool: {e}")
        })
    }
}

impl TryFrom<PoolConfig> for DatabasePool {
    type Error = anyhow::Error;

    fn try_from(config: PoolConfig) -> Result<Self> {
        debug!(database_path = %config.database_path.display(), "Creating database pool");

        // Perform preflight checks before attempting connection
        preflight::run_preflight_checks(&config.database_path)
            .map_err(|e| anyhow::anyhow!(e.with_hint()))?;

        // Use backon for retry logic with exponential backoff
        let retry_strategy = ExponentialBuilder::default()
            .with_min_delay(Duration::from_secs(1))
            .with_max_delay(Duration::from_secs(4))
            .with_max_times(3)
            .with_jitter();

        // Retry pool creation with notification on each retry
        let pool = (|| Self::try_create_pool(&config))
            .retry(&retry_strategy)
            .notify(|err: &anyhow::Error, duration: Duration| {
                warn!(
                    error = %err,
                    backoff_secs = duration.as_secs(),
                    "Failed to create connection pool, retrying..."
                );
            })
            .call()
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to create database connection pool after retries: {}\n\
                     \n\
                     Troubleshooting steps:\n\
                     1. Check if the database file is corrupted:\n\
                        sqlite3 '{}' \"PRAGMA integrity_check;\"\n\
                     2. Check available disk space:\n\
                        df -h\n\
                     3. Check file permissions:\n\
                        ls -la '{}'\n\
                     4. Check for file locks:\n\
                        lsof | grep '{}'\n\
                     5. Verify filesystem supports WAL mode (not all network filesystems do)",
                    e,
                    config.database_path.display(),
                    config.database_path.display(),
                    config.database_path.display()
                )
            })?;

        debug!(
            database_path = %config.database_path.display(),
            "Successfully created connection pool"
        );

        Ok(pool)
    }
}

impl DatabasePool {
    /// Attempts to create a database pool without retry logic
    ///
    /// # Errors
    /// Returns an error if pool creation, connection acquisition, or migration
    /// fails
    fn try_create_pool(config: &PoolConfig) -> Result<Self> {
        let database_url = config.database_path.to_string_lossy().to_string();
        let manager = ConnectionManager::<SqliteConnection>::new(&database_url);

        // Configure SQLite for better concurrency ref: https://docs.diesel.rs/master/diesel/sqlite/struct.SqliteConnection.html#concurrency
        #[derive(Debug)]
        struct SqliteCustomizer;
        impl CustomizeConnection<SqliteConnection, diesel::r2d2::Error> for SqliteCustomizer {
            fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), diesel::r2d2::Error> {
                diesel::sql_query("PRAGMA busy_timeout = 30000;")
                    .execute(conn)
                    .map_err(diesel::r2d2::Error::QueryError)?;
                diesel::sql_query("PRAGMA journal_mode = WAL;")
                    .execute(conn)
                    .map_err(diesel::r2d2::Error::QueryError)?;
                diesel::sql_query("PRAGMA synchronous = NORMAL;")
                    .execute(conn)
                    .map_err(diesel::r2d2::Error::QueryError)?;
                diesel::sql_query("PRAGMA wal_autocheckpoint = 1000;")
                    .execute(conn)
                    .map_err(diesel::r2d2::Error::QueryError)?;
                Ok(())
            }
        }

        let customizer = SqliteCustomizer;

        let mut builder = Pool::builder()
            .max_size(config.max_size)
            .connection_timeout(config.connection_timeout)
            .connection_customizer(Box::new(customizer));

        if let Some(min_idle) = config.min_idle {
            builder = builder.min_idle(Some(min_idle));
        }

        if let Some(idle_timeout) = config.idle_timeout {
            builder = builder.idle_timeout(Some(idle_timeout));
        }

        let pool = builder.build(manager).map_err(|e| {
            warn!(error = %e, "Failed to create connection pool");
            anyhow::anyhow!("Failed to create connection pool: {e}")
        })?;

        // Run migrations on a connection from the pool
        let mut connection = pool
            .get()
            .map_err(|e| anyhow::anyhow!("Failed to get connection for migrations: {e}"))?;

        connection.run_pending_migrations(MIGRATIONS).map_err(|e| {
            warn!(error = %e, "Failed to run database migrations");
            anyhow::anyhow!("Failed to run database migrations: {e}")
        })?;

        Ok(Self { pool })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_pool_creation_with_valid_conditions() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let config = PoolConfig::new(db_path);

        let result = DatabasePool::try_from(config);

        assert!(
            result.is_ok(),
            "Pool creation should succeed with valid conditions"
        );
    }

    #[test]
    fn test_pool_creation_fails_with_readonly_directory() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("readonly_dir").join("test.db");

        // Create readonly directory
        fs::create_dir(temp_dir.path().join("readonly_dir")).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(temp_dir.path().join("readonly_dir"))
                .unwrap()
                .permissions();
            perms.set_mode(0o444); // Read-only
            fs::set_permissions(temp_dir.path().join("readonly_dir"), perms).unwrap();
        }

        let config = PoolConfig::new(db_path.clone());
        let result = DatabasePool::try_from(config);

        // On Unix systems this should fail with permission error
        #[cfg(unix)]
        {
            assert!(
                result.is_err(),
                "Pool creation should fail with readonly directory"
            );
            let error_msg = result.unwrap_err().to_string();
            assert!(
                error_msg.contains("Permission denied") || error_msg.contains("read-only"),
                "Error should mention permission denied or read-only: {}",
                error_msg
            );
        }

        // Clean up - restore permissions so tempdir can be deleted
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(temp_dir.path().join("readonly_dir"))
                .unwrap()
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(temp_dir.path().join("readonly_dir"), perms).unwrap();
        }
    }

    #[test]
    fn test_pool_creation_with_nonexistent_parent() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir
            .path()
            .join("nonexistent")
            .join("deeply")
            .join("nested")
            .join("test.db");

        let config = PoolConfig::new(db_path);

        let result = DatabasePool::try_from(config);

        // Should succeed because preflight checks create parent directories
        assert!(
            result.is_ok(),
            "Pool creation should succeed and create parent directories"
        );
    }

    #[test]
    fn test_error_message_includes_troubleshooting() {
        let config = PoolConfig::new(PathBuf::from("/definitely/does/not/exist/test.db"));

        let result = DatabasePool::try_from(config);

        assert!(
            result.is_err(),
            "Pool creation should fail with invalid path"
        );

        let error_msg = result.unwrap_err().to_string();

        // Verify error message includes troubleshooting guidance
        assert!(
            error_msg.contains("Action:") || error_msg.contains("Try:"),
            "Error message should include actionable guidance: {}",
            error_msg
        );
    }
}

