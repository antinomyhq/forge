use anyhow::Result;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use std::path::PathBuf;
use std::time::Duration;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

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

pub struct DatabasePool {
    pool: DbPool,
}

impl DatabasePool {
    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        let manager = ConnectionManager::<SqliteConnection>::new(":memory:");
        
        let pool = Pool::builder()
            .max_size(1) // Single connection for in-memory testing
            .connection_timeout(Duration::from_secs(30))
            .build(manager)
            .map_err(|e| anyhow::anyhow!("Failed to create in-memory connection pool: {}", e))?;

        // Run migrations on the in-memory database
        let mut connection = pool
            .get()
            .map_err(|e| anyhow::anyhow!("Failed to get connection for migrations: {}", e))?;

        connection
            .run_pending_migrations(MIGRATIONS)
            .map_err(|e| anyhow::anyhow!("Failed to run database migrations: {}", e))?;

        Ok(Self { pool })
    }

    pub fn get_connection(&self) -> Result<PooledSqliteConnection> {
        self.pool
            .get()
            .map_err(|e| anyhow::anyhow!("Failed to get connection from pool: {}", e))
    }
}

impl TryFrom<PoolConfig> for DatabasePool {
    type Error = anyhow::Error;

    fn try_from(config: PoolConfig) -> Result<Self> {
        // Ensure the parent directory exists
        if let Some(parent) = config.database_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let database_url = config.database_path.to_string_lossy().to_string();
        let manager = ConnectionManager::<SqliteConnection>::new(&database_url);

        let mut builder = Pool::builder()
            .max_size(config.max_size)
            .connection_timeout(config.connection_timeout);

        if let Some(min_idle) = config.min_idle {
            builder = builder.min_idle(Some(min_idle));
        }

        if let Some(idle_timeout) = config.idle_timeout {
            builder = builder.idle_timeout(Some(idle_timeout));
        }

        let pool = builder
            .build(manager)
            .map_err(|e| anyhow::anyhow!("Failed to create connection pool: {}", e))?;

        // Run migrations on a connection from the pool
        let mut connection = pool
            .get()
            .map_err(|e| anyhow::anyhow!("Failed to get connection for migrations: {}", e))?;

        connection
            .run_pending_migrations(MIGRATIONS)
            .map_err(|e| anyhow::anyhow!("Failed to run database migrations: {}", e))?;

        Ok(Self { pool })
    }
}
