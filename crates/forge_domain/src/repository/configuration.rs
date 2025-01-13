use async_trait::async_trait;

use crate::Config;

#[async_trait]
pub trait ConfigurationRepository {
    /// Get the current configuration
    async fn get_configuration(&self) -> anyhow::Result<Config>;
    
    /// Save a new configuration
    async fn save_configuration(&self, config: &Config) -> anyhow::Result<()>;
}