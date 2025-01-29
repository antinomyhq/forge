use anyhow::{Context, Result};
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use tracing::debug;

use super::conn::ConnectionOptions;
use super::Sqlite;

type SQLConnection = Pool<ConnectionManager<SqliteConnection>>;

const DB_NAME: &str = "forge.db";
const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

/// SQLite driver that manages database connections and migrations
#[derive(Debug)]
pub(crate) struct Driver {
    pool: SQLConnection,
}

impl Driver {
    pub fn new(db_path: &str) -> Result<Self> {
        let db_path = format!("{}/{}", db_path, DB_NAME);

        // Run migrations first
        let mut conn = SqliteConnection::establish(&db_path)
            .with_context(|| format!("Failed to establish db connection on {}", db_path))?;
        let migrations = conn
            .run_pending_migrations(MIGRATIONS)
            .map_err(|e| anyhow::anyhow!("Database initialization failed with error: {}", e))
            .with_context(|| format!("Failed to run database migrations on {}", db_path))?;

        debug!(
            "Running {} migrations for database: {}",
            migrations.len(),
            db_path
        );

        drop(conn);

        // Create connection pool with default options
        let manager = ConnectionManager::new(db_path);
        let options = ConnectionOptions::default();

        let pool = Pool::builder()
            .connection_customizer(Box::new(options.clone()))
            .max_size(options.max_connections)
            .connection_timeout(options.connection_timeout)
            .test_on_check_out(true)
            .build(manager)?;

        Ok(Driver { pool })
    }
}

#[async_trait::async_trait]
impl Sqlite for Driver {
    async fn connection(&self) -> Result<PooledConnection<ConnectionManager<SqliteConnection>>> {
        self.pool.get().with_context(|| {
            "Failed to acquire connection from pool - pool may be exhausted or database locked"
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use tempfile::TempDir;

    use super::*;

    /// Test driver that handles temporary database creation and cleanup
    pub struct TestDriver {
        driver: Driver,
        // Keep TempDir alive for the duration of the test
        _temp_dir: TempDir,
    }

    impl TestDriver {
        pub fn new() -> Result<Self> {
            let temp_dir = TempDir::new().unwrap();
            let db_path = temp_dir.path().to_str().unwrap().to_string();

            Ok(Self { driver: Driver::new(&db_path)?, _temp_dir: temp_dir })
        }
    }

    #[async_trait::async_trait]
    impl Sqlite for TestDriver {
        async fn connection(
            &self,
        ) -> Result<PooledConnection<ConnectionManager<SqliteConnection>>> {
            self.driver.connection().await
        }
    }

    #[tokio::test]
    async fn test_connection() -> Result<()> {
        let sqlite = TestDriver::new()?;
        let mut conn = sqlite.connection().await?;
        // Verify we can execute a simple query
        diesel::sql_query("SELECT 1").execute(&mut conn)?;
        Ok(())
    }

    #[tokio::test]
    async fn test_multiple_connections() -> Result<()> {
        let sqlite = TestDriver::new()?;

        // Get two connections and verify they both work
        let mut conn1 = sqlite.connection().await?;
        let mut conn2 = sqlite.connection().await?;

        diesel::sql_query("SELECT 1").execute(&mut conn1)?;
        diesel::sql_query("SELECT 1").execute(&mut conn2)?;

        Ok(())
    }
}
