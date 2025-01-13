use async_trait::async_trait;
use crate::Environment;

/// Repository for accessing system environment information
#[async_trait]
pub trait EnvironmentRepository {
    /// Get the current environment information including:
    /// - Operating system
    /// - Current working directory
    /// - Home directory
    /// - Default shell
    async fn get_environment(&self) -> anyhow::Result<Environment>;
}