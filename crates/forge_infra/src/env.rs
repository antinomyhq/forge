use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use forge_app::EnvironmentInfra;
use forge_config::{ConfigReader, ForgeConfig, ModelConfig};
use forge_domain::{ConfigOperation, Environment};
use tracing::{debug, error};

/// Builds a [`forge_domain::Environment`] from runtime context only.
///
/// Only the six fields that cannot be sourced from [`ForgeConfig`] are set
/// here: `os`, `pid`, `cwd`, `home`, `shell`, and `base_path`. All
/// configuration values are now accessed through
/// `EnvironmentInfra::get_config()`.
fn to_environment(cwd: PathBuf) -> Environment {
    Environment {
        os: std::env::consts::OS.to_string(),
        pid: std::process::id(),
        cwd,
        home: dirs::home_dir(),
        shell: if cfg!(target_os = "windows") {
            std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
        } else {
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
        },
        base_path: dirs::home_dir()
            .map(|h| h.join("forge"))
            .unwrap_or_else(|| PathBuf::from(".").join("forge")),
    }
}

/// Applies a single [`ConfigOperation`] directly to a [`ForgeConfig`].
///
/// Used by [`ForgeEnvironmentInfra::update_environment`] to mutate the
/// persisted config without an intermediate `Environment` round-trip.
fn apply_config_op(fc: &mut ForgeConfig, op: ConfigOperation) {
    match op {
        ConfigOperation::SetProvider(pid) => {
            let session = fc.session.get_or_insert_with(ModelConfig::default);
            session.provider_id = Some(pid.as_ref().to_string());
        }
        ConfigOperation::SetModel(pid, mid) => {
            let pid_str = pid.as_ref().to_string();
            let mid_str = mid.to_string();
            let session = fc.session.get_or_insert_with(ModelConfig::default);
            if session.provider_id.as_deref() == Some(&pid_str) {
                session.model_id = Some(mid_str);
            } else {
                fc.session =
                    Some(ModelConfig { provider_id: Some(pid_str), model_id: Some(mid_str) });
            }
        }
        ConfigOperation::SetCommitConfig(commit) => {
            fc.commit = commit
                .provider
                .as_ref()
                .zip(commit.model.as_ref())
                .map(|(pid, mid)| ModelConfig {
                    provider_id: Some(pid.as_ref().to_string()),
                    model_id: Some(mid.to_string()),
                });
        }
        ConfigOperation::SetSuggestConfig(suggest) => {
            fc.suggest = Some(ModelConfig {
                provider_id: Some(suggest.provider.as_ref().to_string()),
                model_id: Some(suggest.model.to_string()),
            });
        }
        ConfigOperation::SetReasoningEffort(effort) => {
            let config_effort = match effort {
                forge_domain::Effort::None => forge_config::Effort::None,
                forge_domain::Effort::Minimal => forge_config::Effort::Minimal,
                forge_domain::Effort::Low => forge_config::Effort::Low,
                forge_domain::Effort::Medium => forge_config::Effort::Medium,
                forge_domain::Effort::High => forge_config::Effort::High,
                forge_domain::Effort::XHigh => forge_config::Effort::XHigh,
                forge_domain::Effort::Max => forge_config::Effort::Max,
            };
            let reasoning = fc
                .reasoning
                .get_or_insert_with(forge_config::ReasoningConfig::default);
            reasoning.effort = Some(config_effort);
        }
    }
}

/// Infrastructure implementation for managing application configuration with
/// caching support.
///
/// Uses [`ForgeConfig::read`] and [`ForgeConfig::write`] for all file I/O and
/// maintains an in-memory cache to reduce disk access. Also handles
/// environment variable discovery via `.env` files and OS APIs.
pub struct ForgeEnvironmentInfra {
    cwd: PathBuf,
    cache: Arc<std::sync::Mutex<Option<ForgeConfig>>>,
}

impl ForgeEnvironmentInfra {
    /// Creates a new [`ForgeEnvironmentInfra`].
    ///
    /// # Arguments
    /// * `cwd` - The working directory path; used to resolve `.env` files
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd, cache: Arc::new(std::sync::Mutex::new(None)) }
    }

    /// Reads [`ForgeConfig`] from disk via [`ForgeConfig::read`].
    fn read_from_disk() -> ForgeConfig {
        match ForgeConfig::read() {
            Ok(config) => {
                debug!(config = ?config, "read .forge.toml");
                config
            }
            Err(e) => {
                // NOTE: This should never-happen
                error!(error = ?e, "Failed to read config file. Using default config.");
                Default::default()
            }
        }
    }

    /// Returns the cached [`ForgeConfig`], reading from disk if the cache is
    /// empty.
    fn cached_config(&self) -> ForgeConfig {
        let mut cache = self.cache.lock().expect("cache mutex poisoned");
        if let Some(ref config) = *cache {
            config.clone()
        } else {
            let config = Self::read_from_disk();
            *cache = Some(config.clone());
            config
        }
    }
}

impl EnvironmentInfra for ForgeEnvironmentInfra {
    type Config = ForgeConfig;

    fn get_env_var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }

    fn get_env_vars(&self) -> BTreeMap<String, String> {
        std::env::vars().collect()
    }

    fn get_environment(&self) -> Environment {
        to_environment(self.cwd.clone())
    }

    fn get_config(&self) -> ForgeConfig {
        self.cached_config()
    }

    async fn update_environment(&self, ops: Vec<ConfigOperation>) -> anyhow::Result<()> {
        // Load the global config (with defaults applied) for the update round-trip
        let mut fc = ConfigReader::default()
            .read_defaults()
            .read_global()
            .build()?;

        debug!(config = ?fc, ?ops, "applying app config operations");

        for op in ops {
            apply_config_op(&mut fc, op);
        }

        fc.write()?;
        debug!(config = ?fc, "written .forge.toml");

        // Reset cache
        *self.cache.lock().expect("cache mutex poisoned") = None;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use forge_config::ForgeConfig;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_to_environment_sets_cwd() {
        let fixture_cwd = PathBuf::from("/test/cwd");
        let actual = to_environment(fixture_cwd.clone());
        assert_eq!(actual.cwd, fixture_cwd);
    }

    #[test]
    fn test_apply_config_op_set_provider() {
        use forge_domain::ProviderId;

        let mut fixture = ForgeConfig::default();
        apply_config_op(
            &mut fixture,
            ConfigOperation::SetProvider(ProviderId::ANTHROPIC),
        );

        let actual = fixture
            .session
            .as_ref()
            .and_then(|s| s.provider_id.as_deref());
        let expected = Some("anthropic");

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_apply_config_op_set_model_matching_provider() {
        use forge_domain::{ModelId, ProviderId};

        let mut fixture = ForgeConfig {
            session: Some(ModelConfig {
                provider_id: Some("anthropic".to_string()),
                model_id: None,
            }),
            ..Default::default()
        };

        apply_config_op(
            &mut fixture,
            ConfigOperation::SetModel(
                ProviderId::ANTHROPIC,
                ModelId::new("claude-3-5-sonnet-20241022"),
            ),
        );

        let actual = fixture.session.as_ref().and_then(|s| s.model_id.as_deref());
        let expected = Some("claude-3-5-sonnet-20241022");

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_apply_config_op_set_model_different_provider_replaces_session() {
        use forge_domain::{ModelId, ProviderId};

        let mut fixture = ForgeConfig {
            session: Some(ModelConfig {
                provider_id: Some("openai".to_string()),
                model_id: Some("gpt-4".to_string()),
            }),
            ..Default::default()
        };

        apply_config_op(
            &mut fixture,
            ConfigOperation::SetModel(
                ProviderId::ANTHROPIC,
                ModelId::new("claude-3-5-sonnet-20241022"),
            ),
        );

        let actual_provider = fixture
            .session
            .as_ref()
            .and_then(|s| s.provider_id.as_deref());
        let actual_model = fixture.session.as_ref().and_then(|s| s.model_id.as_deref());

        assert_eq!(actual_provider, Some("anthropic"));
        assert_eq!(actual_model, Some("claude-3-5-sonnet-20241022"));
    }
}
