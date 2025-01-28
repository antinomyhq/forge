mod conn;
pub(crate) mod driver;

use anyhow::Result;
use diesel::r2d2::{ConnectionManager, PooledConnection};
use diesel::sqlite::SqliteConnection;
#[cfg(test)]
pub(crate) use driver::tests::TestDriver;
pub(crate) use driver::Driver;

use crate::service::Service;

/// Sqlite provides database access functionality.
/// It abstracts away the connection pooling details and
/// provides a simple interface for getting database connections.
#[async_trait::async_trait]
pub trait Sqlite: Send + Sync {
    /// Gets a connection from the pool.
    async fn connection(&self) -> Result<PooledConnection<ConnectionManager<SqliteConnection>>>;
}

impl Service {
    /// Create a new SQLite service
    pub fn db_pool_service(forge_config_dir: &str) -> Result<impl Sqlite + 'static> {
        Driver::new(forge_config_dir)
    }
}
