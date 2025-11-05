use merge::Merge;
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum_macros::{EnumDiscriminants, EnumIter};

use crate::{AgentId, ProviderId};

#[derive(Clone)]
pub struct Trace<Config> {
    trace: Vec<(String, bool)>,
    value: Config,
}

impl<Config> Trace<Config> {
    /// Creates a new trace with a single entry
    pub fn new(scope: impl Into<SimpleConfigScope>, config: Config) -> Self {
        let scope: SimpleConfigScope = scope.into();
        let trace = SimpleConfigScope::iter()
            .map(|a| {
                if a == scope {
                    (a.to_string(), true)
                } else {
                    (a.to_string(), false)
                }
            })
            .collect::<Vec<_>>();
        Self { trace, value: config }
    }

    pub fn into_value(self) -> Config {
        self.value
    }

    pub fn trace(&self) -> &[(String, bool)] {
        &self.trace
    }
}

/// Represents the scope at which a
/// configuration value should be resolved or
/// set.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, EnumDiscriminants)]
#[strum_discriminants(derive(EnumIter, strum_macros::Display))]
#[strum_discriminants(name(SimpleConfigScope))]
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

    async fn get_global_level(&self) -> anyhow::Result<Option<Trace<Self::Config>>>;
    async fn get_project_level(&self) -> anyhow::Result<Option<Trace<Self::Config>>>;
    async fn get_provider_level(
        &self,
        id: &ProviderId,
    ) -> anyhow::Result<Option<Trace<Self::Config>>>;
    async fn get_agent_level(&self, id: &AgentId) -> anyhow::Result<Option<Trace<Self::Config>>>;

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
    pub async fn get<T>(&self, resolver: &T) -> anyhow::Result<Option<Trace<T::Config>>>
    where
        T: ScopeResolution + Send + Sync,
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
    pub async fn set<T>(&self, resolver: &T, config: T::Config) -> anyhow::Result<Option<()>>
    where
        T: ScopeResolution + Send + Sync,
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

    pub async fn merged<T>(&self, resolver: &T) -> anyhow::Result<Option<T::Config>>
    where
        T: ScopeResolution,
        T::Config: Merge,
    {
        match self {
            ConfigScope::Global => Ok(resolver.get_global_level().await?.map(Trace::into_value)),
            ConfigScope::Project => Ok(resolver.get_project_level().await?.map(Trace::into_value)),
            ConfigScope::Provider(id) => Ok(resolver
                .get_provider_level(id)
                .await?
                .map(Trace::into_value)),
            ConfigScope::Agent(id) => {
                Ok(resolver.get_agent_level(id).await?.map(Trace::into_value))
            }
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
    pub async fn all<T>(&self, resolver: &T) -> anyhow::Result<Vec<T::Config>>
    where
        T: ScopeResolution + Send + Sync,
        T::Config: Send + Sync,
    {
        match self {
            ConfigScope::Global => Ok(resolver
                .get_global_level()
                .await?
                .map(Trace::into_value)
                .into_iter()
                .collect()),
            ConfigScope::Project => Ok(resolver
                .get_project_level()
                .await?
                .map(Trace::into_value)
                .into_iter()
                .collect()),
            ConfigScope::Provider(id) => Ok(resolver
                .get_provider_level(id)
                .await?
                .map(Trace::into_value)
                .into_iter()
                .collect()),
            ConfigScope::Agent(id) => Ok(resolver
                .get_agent_level(id)
                .await?
                .map(Trace::into_value)
                .into_iter()
                .collect()),
            ConfigScope::Or(a, b) => {
                let a_side = a.all(resolver).await?;
                let b_side = b.all(resolver).await?;
                Ok(a_side.into_iter().chain(b_side.into_iter()).collect())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_trace_with_single_entry() {
        let fixture = Trace::new(ConfigScope::Global, 42);
        let actual = fixture.into_value();
        let expected = 42;
        assert_eq!(actual, expected);
    }
}
