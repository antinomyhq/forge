use crate::{AgentId, ProviderId};
use merge::Merge;
use serde::{Deserialize, Serialize};

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

impl ConfigScope {
    pub fn or(self, b: ConfigScope) -> ConfigScope {
        ConfigScope::Or(Box::new(self), Box::new(b))
    }
}

/// Trait for resolving and setting configuration values at different scopes.
#[async_trait::async_trait]
pub trait ScopeResolution {
    type Config;

    async fn get_global_level(&self) -> anyhow::Result<Option<Self::Config>>;
    async fn get_project_level(&self) -> anyhow::Result<Option<Self::Config>>;
    async fn get_provider_level(&self, id: &ProviderId) -> anyhow::Result<Option<Self::Config>>;
    async fn get_agent_level(&self, id: &AgentId) -> anyhow::Result<Option<Self::Config>>;

    async fn set_global_level(&self, config: Self::Config) -> anyhow::Result<Option<()>>;
    async fn set_project_level(&self, config: Self::Config) -> anyhow::Result<Option<()>>;
    async fn set_provider_level(
        &self,
        id: &ProviderId,
        config: Self::Config,
    ) -> anyhow::Result<Option<()>>;
    async fn set_agent_level(
        &self,
        id: &AgentId,
        config: Self::Config,
    ) -> anyhow::Result<Option<()>>;
}

impl ConfigScope {
    #[async_recursion::async_recursion]
    pub async fn get<T: ScopeResolution + Send + Sync>(
        &self,
        resolver: &T,
    ) -> anyhow::Result<Option<T::Config>>
    where
        T::Config: Send + Sync,
    {
        match self {
            ConfigScope::Global => resolver.get_global_level().await,
            ConfigScope::Project => resolver.get_project_level().await,
            ConfigScope::Provider(id) => resolver.get_provider_level(id).await,
            ConfigScope::Agent(id) => resolver.get_agent_level(id).await,
            ConfigScope::Or(a, b) => match a.get(resolver).await? {
                Some(value) => Ok(Some(value)),
                None => b.get(resolver).await,
            },
        }
    }

    #[async_recursion::async_recursion]
    pub async fn set<T: ScopeResolution + Send + Sync>(
        &self,
        resolver: &T,
        config: T::Config,
    ) -> anyhow::Result<Option<()>>
    where
        T::Config: Send + Sync + Clone,
    {
        match self {
            ConfigScope::Global => resolver.set_global_level(config).await,
            ConfigScope::Project => resolver.set_project_level(config).await,
            ConfigScope::Provider(id) => resolver.set_provider_level(id, config).await,
            ConfigScope::Agent(id) => resolver.set_agent_level(id, config).await,
            ConfigScope::Or(a, b) => match a.set(resolver, config.clone()).await {
                Ok(Some(_)) => Ok(Some(())),
                Ok(None) => b.set(resolver, config).await,
                Err(e) => Err(e),
            },
        }
    }

    pub async fn merged<T: ScopeResolution>(
        &self,
        resolver: &T,
    ) -> anyhow::Result<Option<T::Config>>
    where
        T::Config: Merge,
    {
        match self {
            ConfigScope::Global => resolver.get_global_level().await,
            ConfigScope::Project => resolver.get_project_level().await,
            ConfigScope::Provider(id) => resolver.get_provider_level(id).await,
            ConfigScope::Agent(id) => resolver.get_agent_level(id).await,
            ConfigScope::Or(a, b) => {
                let a = a.merged(resolver).await?;
                let b = b.merged(resolver).await?;
                match (a, b) {
                    (Some(mut a), Some(b)) => {
                        a.merge(b);
                        Ok(Some(a))
                    }
                    (Some(a), _) => Ok(Some(a)),
                    (_, Some(b)) => Ok(Some(b)),
                    (None, None) => Ok(None),
                }
            }
        }
    }

    #[async_recursion::async_recursion]
    pub async fn all<T: ScopeResolution + Send + Sync>(
        &self,
        resolver: &T,
    ) -> anyhow::Result<Vec<T::Config>>
    where
        T::Config: Send + Sync,
    {
        match self {
            ConfigScope::Global => Ok(resolver.get_global_level().await?.into_iter().collect()),
            ConfigScope::Project => Ok(resolver.get_project_level().await?.into_iter().collect()),
            ConfigScope::Provider(id) => {
                Ok(resolver.get_provider_level(id).await?.into_iter().collect())
            }
            ConfigScope::Agent(id) => Ok(resolver.get_agent_level(id).await?.into_iter().collect()),
            ConfigScope::Or(a, b) => {
                let a_side = a.all(resolver).await?;
                let b_side = b.all(resolver).await?;
                Ok(a_side.into_iter().chain(b_side.into_iter()).collect())
            }
        }
    }
}
