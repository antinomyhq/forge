use anyhow::Result;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use diesel::prelude::*;
use tracing::debug;

pub type DbPool = Pool<ConnectionManager<SqliteConnection>>;
pub type PooledSqliteConnection = PooledConnection<ConnectionManager<SqliteConnection>>;

#[derive(Debug)]
pub struct DatabasePool {
    pool: DbPool,
}

impl DatabasePool {
    pub fn get_connection(&self) -> Result<PooledSqliteConnection, anyhow::Error> {
        self.pool.get().map_err(|e| {
            anyhow::anyhow!("Failed to get connection from pool: {e}")
        })
    }

    pub fn in_memory() -> Result<Self, anyhow::Error> {
        let pool = DbPool::builder()
            .max_size(10)
            .build(ConnectionManager::<SqliteConnection>::new(":memory:"))
            .map_err(|e| anyhow::anyhow!("Failed to create in-memory pool: {e}"))?;

        let mut conn = pool.get()?;
        Self::setup_connection(&mut conn)?;
        Self::run_migrations(&mut conn)?;

        Ok(Self { pool })
    }
}

impl TryFrom<PoolConfig> for DatabasePool {
    type Error = anyhow::Error;

    fn try_from(config: PoolConfig) -> Result<Self, anyhow::Error> {
        debug!(database_path = %config.database_path.display(), "Creating database pool");

        // Ensure the parent directory exists
        if let Some(parent) = config.database_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let pool = DbPool::builder()
            .max_size(config.max_connections)
            .build(ConnectionManager::new(config.database_path.to_string_lossy().to_string()))
            .map_err(|e| anyhow::anyhow!("Failed to create database pool: {e}"))?;

        let mut conn = pool.get()?;
        Self::setup_connection(&mut conn)?;
        Self::run_migrations(&mut conn)?;

        Ok(Self { pool })
    }
}

impl DatabasePool {
    fn setup_connection(conn: &mut SqliteConnection) -> Result<(), anyhow::Error> {
        debug!("Setting up database connection");
        diesel::sql_query("PRAGMA busy_timeout = 30000;")
            .execute(conn)?;
        diesel::sql_query("PRAGMA journal_mode = WAL;")
            .execute(conn)?;
        diesel::sql_query("PRAGMA synchronous = NORMAL;")
            .execute(conn)?;
        diesel::sql_query("PRAGMA wal_autocheckpoint = 1000;")
            .execute(conn)?;
        Ok(())
    }

    fn run_migrations(conn: &mut SqliteConnection) -> Result<(), anyhow::Error> {
        const MIGRATIONS: EmbeddedMigrations = embed_migrations!("src/database/migrations");
        debug!("Running database migrations");
        conn.run_pending_migrations(MIGRATIONS)
            .map_err(|e| anyhow::anyhow!("Failed to run migrations: {e}"))?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct PoolConfig {
    pub database_path: std::path::PathBuf,
    pub max_connections: u32,
}

impl PoolConfig {
    pub fn new(database_path: std::path::PathBuf) -> Self {
        Self {
            database_path,
            max_connections: 10,
        }
    }
}