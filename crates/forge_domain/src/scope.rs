use merge::Merge;
use serde::{Deserialize, Serialize};

use crate::{AgentId, ProviderId};

#[derive(Clone)]
pub struct Trace<T> {
    path: Vec<String>,
    value: T,
}

impl<T> Trace<T> {
    /// Creates a new trace with empty path
    pub fn new(value: T) -> Self {
        Self { path: Vec::new(), value }
    }

    /// Appends a scope to the trace path
    pub fn add_trace(mut self, scope: impl ToString) -> Self {
        self.path.push(scope.to_string());
        self
    }

    pub fn into_inner(self) -> T {
        self.value
    }

    pub fn trace(&self) -> &[String] {
        &self.path
    }
}

/// Represents the scope at which a
/// configuration value should be resolved or
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

/// Trait for getting configuration values at different scopes.
#[async_trait::async_trait]
pub trait ScopeGetter {
    type Config;

    async fn get_global_level(&self) -> anyhow::Result<Option<Self::Config>>;
    async fn get_project_level(&self) -> anyhow::Result<Option<Self::Config>>;
    async fn get_provider_level(&self, id: &ProviderId) -> anyhow::Result<Option<Self::Config>>;
    async fn get_agent_level(&self, id: &AgentId) -> anyhow::Result<Option<Self::Config>>;
}

/// Trait for setting configuration values at different scopes.
#[async_trait::async_trait]
pub trait ScopeSetter {
    type Config;

    async fn set_global_level(&self, config: Self::Config) -> anyhow::Result<bool>;
    async fn set_project_level(&self, config: Self::Config) -> anyhow::Result<bool>;
    async fn set_provider_level(
        &self,
        id: &ProviderId,
        config: Self::Config,
    ) -> anyhow::Result<bool>;
    async fn set_agent_level(&self, id: &AgentId, config: Self::Config) -> anyhow::Result<bool>;
}

/// Extension methods for types implementing ScopeGetter
#[async_trait::async_trait]
pub trait ScopeGetterExt: ScopeGetter {
    /// Gets a configuration value at the specified scope with tracing
    async fn get_at_scope(&self, scope: &ConfigScope) -> anyhow::Result<Option<Trace<Self::Config>>>
    where
        Self: Send + Sync,
        Self::Config: Send + Sync,
    {
        match scope {
            ConfigScope::Global => Ok(self
                .get_global_level()
                .await?
                .map(|v| Trace::new(v).add_trace("global"))),
            ConfigScope::Project => Ok(self
                .get_project_level()
                .await?
                .map(|v| Trace::new(v).add_trace("project"))),
            ConfigScope::Provider(id) => Ok(self
                .get_provider_level(id)
                .await?
                .map(|v| Trace::new(v).add_trace("provider"))),
            ConfigScope::Agent(id) => Ok(self
                .get_agent_level(id)
                .await?
                .map(|v| Trace::new(v).add_trace("agent"))),
            ConfigScope::Or(a, b) => match Box::pin(self.get_at_scope(a)).await? {
                Some(value) => Ok(Some(value)),
                None => Box::pin(self.get_at_scope(b)).await,
            },
        }
    }

    /// Gets all configuration values at the specified scope
    async fn get_all_at_scope(&self, scope: &ConfigScope) -> anyhow::Result<Vec<Self::Config>>
    where
        Self: Send + Sync,
        Self::Config: Send + Sync,
    {
        match scope {
            ConfigScope::Global => Ok(self.get_global_level().await?.into_iter().collect()),
            ConfigScope::Project => Ok(self.get_project_level().await?.into_iter().collect()),
            ConfigScope::Provider(id) => {
                Ok(self.get_provider_level(id).await?.into_iter().collect())
            }
            ConfigScope::Agent(id) => Ok(self.get_agent_level(id).await?.into_iter().collect()),
            ConfigScope::Or(a, b) => {
                let a_side = Box::pin(self.get_all_at_scope(a)).await?;
                let b_side = Box::pin(self.get_all_at_scope(b)).await?;
                Ok(a_side.into_iter().chain(b_side).collect())
            }
        }
    }

    /// Gets merged configuration values at the specified scope
    async fn get_merged_at_scope(&self, scope: &ConfigScope) -> anyhow::Result<Option<Self::Config>>
    where
        Self::Config: Merge + Send,
    {
        match scope {
            ConfigScope::Global => self.get_global_level().await,
            ConfigScope::Project => self.get_project_level().await,
            ConfigScope::Provider(id) => self.get_provider_level(id).await,
            ConfigScope::Agent(id) => self.get_agent_level(id).await,
            ConfigScope::Or(a, b) => {
                let a_val = Box::pin(self.get_merged_at_scope(a)).await?;
                let b_val = Box::pin(self.get_merged_at_scope(b)).await?;
                match (a_val, b_val) {
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
}

// Blanket implementation for all types implementing ScopeGetter
impl<T: ScopeGetter> ScopeGetterExt for T {}

/// Extension methods for types implementing ScopeSetter
#[async_trait::async_trait]
pub trait ScopeSetterExt: ScopeSetter {
    /// Sets a configuration value at the specified scope with tracing
    async fn set_at_scope(
        &self,
        scope: &ConfigScope,
        config: Trace<Self::Config>,
    ) -> anyhow::Result<bool>
    where
        Self: Send + Sync,
        Self::Config: Send + Sync + Clone,
    {
        match scope {
            ConfigScope::Global => self.set_global_level(config.into_inner()).await,
            ConfigScope::Project => self.set_project_level(config.into_inner()).await,
            ConfigScope::Provider(id) => self.set_provider_level(id, config.into_inner()).await,
            ConfigScope::Agent(id) => self.set_agent_level(id, config.into_inner()).await,
            ConfigScope::Or(a, b) => {
                let inner = config.into_inner();
                if Box::pin(self.set_at_scope(a, Trace::new(inner.clone()))).await? {
                    Ok(true)
                } else {
                    Box::pin(self.set_at_scope(b, Trace::new(inner))).await
                }
            }
        }
    }
}

// Blanket implementation for all types implementing ScopeSetter
impl<T: ScopeSetter> ScopeSetterExt for T {}

impl ConfigScope {
    pub async fn get<T>(&self, resolver: &T) -> anyhow::Result<Option<Trace<T::Config>>>
    where
        T: ScopeGetter + Send + Sync,
        T::Config: Send + Sync,
    {
        resolver.get_at_scope(self).await
    }

    pub async fn set<T>(&self, resolver: &T, config: T::Config) -> anyhow::Result<bool>
    where
        T: ScopeSetter + Send + Sync,
        T::Config: Send + Sync + Clone,
    {
        resolver.set_at_scope(self, Trace::new(config)).await
    }

    pub async fn merged<T>(&self, resolver: &T) -> anyhow::Result<Option<T::Config>>
    where
        T: ScopeGetter + Sync,
        T::Config: Merge + Send,
    {
        resolver.get_merged_at_scope(self).await
    }

    pub async fn all<T>(&self, resolver: &T) -> anyhow::Result<Vec<T::Config>>
    where
        T: ScopeGetter + Send + Sync,
        T::Config: Send + Sync,
    {
        resolver.get_all_at_scope(self).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_trace_with_single_entry() {
        let fixture = Trace::new(42);
        let actual = fixture.into_inner();
        let expected = 42;
        assert_eq!(actual, expected);
    }
}
