use serde::{Deserialize, Serialize};

use crate::{AgentId, ProviderId};

/// Represents the scope at which a configuration value should be resolved or
/// set.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConfigScope {
    Global,
    Project,
    Provider(ProviderId),
    Agent(AgentId),
    Or(Box<ConfigScope>, Box<ConfigScope>),
}

/// Trait for resolving and setting configuration values at different scopes.
#[async_trait::async_trait]
pub trait ResolveScopeConfig {
    type Config;

    /// Retrieves configuration for the given scope.
    async fn get(&self, scope: &ConfigScope) -> anyhow::Result<Option<Self::Config>>;

    /// Sets configuration at the specified scope.
    async fn set(&self, scope: &ConfigScope, config: Self::Config) -> anyhow::Result<()>;
}

