use anyhow::Result;

use crate::ConfigScope;

/// Trait for resolving and setting configuration values at different scopes.
#[async_trait::async_trait]
pub trait ResolveScopeConfig {
    type Config;

    /// Retrieves configuration for the given scope.
    async fn get(&self, scope: &ConfigScope) -> Result<Option<Self::Config>>;

    /// Sets configuration at the specified scope.
    async fn set(&self, scope: &ConfigScope, config: Self::Config) -> Result<()>;
}
