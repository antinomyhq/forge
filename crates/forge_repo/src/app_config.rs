use std::sync::Arc;

use forge_config::{ConfigReader, ForgeConfig, ModelConfig};
use forge_domain::{AppConfigOperation, AppConfigRepository};
use tokio::sync::Mutex;
use tracing::{debug, error};

/// Applies a single [`AppConfigOperation`] directly onto a [`ForgeConfig`]
/// in-place.
fn apply_op(op: AppConfigOperation, fc: &mut ForgeConfig) {
    match op {
        AppConfigOperation::KeyInfo(Some(info)) => {
            fc.api_key = Some(info.api_key);
            fc.api_key_name = Some(info.api_key_name);
            fc.api_key_masked = Some(info.api_key_masked);
            fc.email = info.email;
            fc.name = info.name;
            fc.auth_provider_id = info.auth_provider_id;
        }
        AppConfigOperation::KeyInfo(None) => {
            fc.api_key = None;
            fc.api_key_name = None;
            fc.api_key_masked = None;
            fc.email = None;
            fc.name = None;
            fc.auth_provider_id = None;
        }
        AppConfigOperation::SetProvider(provider_id) => {
            let pid = provider_id.as_ref().to_string();
            fc.session = Some(match fc.session.take() {
                Some(mc) => mc.provider_id(pid),
                None => ModelConfig::default().provider_id(pid),
            });
        }
        AppConfigOperation::SetModel(provider_id, model_id) => {
            let pid = provider_id.as_ref().to_string();
            let mid = model_id.to_string();
            fc.session = Some(match fc.session.take() {
                Some(mc) if mc.provider_id.as_deref() == Some(&pid) => mc.model_id(mid),
                _ => ModelConfig::default().provider_id(pid).model_id(mid),
            });
        }
        AppConfigOperation::SetCommitConfig(commit) => {
            fc.commit = commit
                .provider
                .as_ref()
                .zip(commit.model.as_ref())
                .map(|(pid, mid)| {
                    ModelConfig::default()
                        .provider_id(pid.as_ref().to_string())
                        .model_id(mid.to_string())
                });
        }
        AppConfigOperation::SetSuggestConfig(suggest) => {
            fc.suggest = Some(
                ModelConfig::default()
                    .provider_id(suggest.provider.as_ref().to_string())
                    .model_id(suggest.model.to_string()),
            );
        }
    }
}

/// Repository for managing application configuration with caching support.
///
/// Uses [`ForgeConfig::read`] and [`ForgeConfig::write`] for all file I/O and
/// maintains an in-memory cache to reduce disk access.
pub struct ForgeConfigRepository {
    cache: Arc<Mutex<Option<ForgeConfig>>>,
}

impl ForgeConfigRepository {
    pub fn new() -> Self {
        Self { cache: Arc::new(Mutex::new(None)) }
    }

    /// Reads [`ForgeConfig`] from disk via [`ForgeConfig::read`].
    async fn read(&self) -> ForgeConfig {
        let config = ForgeConfig::read();

        match config {
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
}

#[async_trait::async_trait]
impl AppConfigRepository for ForgeConfigRepository {
    async fn get_app_config(&self) -> anyhow::Result<ForgeConfig> {
        // Check cache first
        let cache = self.cache.lock().await;
        if let Some(ref config) = *cache {
            return Ok(config.clone());
        }
        drop(cache);

        // Cache miss, read from file
        let config = self.read().await;

        let mut cache = self.cache.lock().await;
        *cache = Some(config.clone());

        Ok(config)
    }

    async fn update_app_config(&self, ops: Vec<AppConfigOperation>) -> anyhow::Result<()> {
        // Load the global config
        let mut fc = ConfigReader::default().read_global().build()?;

        debug!(config = ?fc, "loaded config for update");

        // Apply each operation directly onto ForgeConfig
        debug!(?ops, "applying app config operations");
        for op in ops {
            apply_op(op, &mut fc);
        }

        // Persist
        fc.write()?;
        debug!(config = ?fc, "written .forge.toml");

        // Reset cache
        let mut cache = self.cache.lock().await;
        *cache = None;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use forge_config::{ForgeConfig, ModelConfig};
    use forge_domain::{AppConfigOperation, CommitConfig, LoginInfo, ModelId, ProviderId, SuggestConfig};
    use pretty_assertions::assert_eq;

    use super::apply_op;

    // ── apply_op ──────────────────────────────────────────────────────────────

    #[test]
    fn test_apply_op_key_info_some_sets_all_fields() {
        let mut fixture = ForgeConfig::default();
        let login = LoginInfo {
            api_key: "key-123".to_string(),
            api_key_name: "prod".to_string(),
            api_key_masked: "key-***".to_string(),
            email: Some("dev@forge.dev".to_string()),
            name: Some("Bob".to_string()),
            auth_provider_id: Some("google".to_string()),
        };
        apply_op(AppConfigOperation::KeyInfo(Some(login)), &mut fixture);
        let expected = ForgeConfig::default()
            .api_key("key-123".to_string())
            .api_key_name("prod".to_string())
            .api_key_masked("key-***".to_string())
            .email("dev@forge.dev".to_string())
            .name("Bob".to_string())
            .auth_provider_id("google".to_string());
        assert_eq!(fixture, expected);
    }

    #[test]
    fn test_apply_op_key_info_none_clears_all_fields() {
        let mut fixture = ForgeConfig::default()
            .api_key("key-abc".to_string())
            .api_key_name("old".to_string())
            .api_key_masked("old-***".to_string())
            .email("old@example.com".to_string())
            .name("Old Name".to_string())
            .auth_provider_id("github".to_string());
        apply_op(AppConfigOperation::KeyInfo(None), &mut fixture);
        assert_eq!(fixture, ForgeConfig::default());
    }

    #[test]
    fn test_apply_op_set_provider_creates_session_when_absent() {
        let mut fixture = ForgeConfig::default();
        apply_op(
            AppConfigOperation::SetProvider(ProviderId::from("anthropic".to_string())),
            &mut fixture,
        );
        let expected = ForgeConfig {
            session: Some(ModelConfig::default().provider_id("anthropic".to_string())),
            ..Default::default()
        };
        assert_eq!(fixture, expected);
    }

    #[test]
    fn test_apply_op_set_provider_updates_existing_session_keeping_model() {
        let mut fixture = ForgeConfig {
            session: Some(
                ModelConfig::default()
                    .provider_id("openai".to_string())
                    .model_id("gpt-4".to_string()),
            ),
            ..Default::default()
        };
        apply_op(
            AppConfigOperation::SetProvider(ProviderId::from("anthropic".to_string())),
            &mut fixture,
        );
        let expected = ForgeConfig {
            session: Some(
                ModelConfig::default()
                    .provider_id("anthropic".to_string())
                    .model_id("gpt-4".to_string()),
            ),
            ..Default::default()
        };
        assert_eq!(fixture, expected);
    }

    #[test]
    fn test_apply_op_set_model_for_matching_provider_updates_model() {
        let mut fixture = ForgeConfig {
            session: Some(
                ModelConfig::default()
                    .provider_id("openai".to_string())
                    .model_id("gpt-3.5".to_string()),
            ),
            ..Default::default()
        };
        apply_op(
            AppConfigOperation::SetModel(
                ProviderId::from("openai".to_string()),
                ModelId::new("gpt-4"),
            ),
            &mut fixture,
        );
        let expected = ForgeConfig {
            session: Some(
                ModelConfig::default()
                    .provider_id("openai".to_string())
                    .model_id("gpt-4".to_string()),
            ),
            ..Default::default()
        };
        assert_eq!(fixture, expected);
    }

    #[test]
    fn test_apply_op_set_model_for_different_provider_replaces_session() {
        let mut fixture = ForgeConfig {
            session: Some(
                ModelConfig::default()
                    .provider_id("openai".to_string())
                    .model_id("gpt-4".to_string()),
            ),
            ..Default::default()
        };
        apply_op(
            AppConfigOperation::SetModel(
                ProviderId::from("anthropic".to_string()),
                ModelId::new("claude-3"),
            ),
            &mut fixture,
        );
        let expected = ForgeConfig {
            session: Some(
                ModelConfig::default()
                    .provider_id("anthropic".to_string())
                    .model_id("claude-3".to_string()),
            ),
            ..Default::default()
        };
        assert_eq!(fixture, expected);
    }

    #[test]
    fn test_apply_op_set_commit_config() {
        let mut fixture = ForgeConfig::default();
        let commit = CommitConfig::default()
            .provider(ProviderId::from("openai".to_string()))
            .model(ModelId::new("gpt-4o"));
        apply_op(AppConfigOperation::SetCommitConfig(commit), &mut fixture);
        let expected = ForgeConfig {
            commit: Some(
                ModelConfig::default()
                    .provider_id("openai".to_string())
                    .model_id("gpt-4o".to_string()),
            ),
            ..Default::default()
        };
        assert_eq!(fixture, expected);
    }

    #[test]
    fn test_apply_op_set_suggest_config() {
        let mut fixture = ForgeConfig::default();
        let suggest = SuggestConfig {
            provider: ProviderId::from("anthropic".to_string()),
            model: ModelId::new("claude-3-haiku"),
        };
        apply_op(AppConfigOperation::SetSuggestConfig(suggest), &mut fixture);
        let expected = ForgeConfig {
            suggest: Some(
                ModelConfig::default()
                    .provider_id("anthropic".to_string())
                    .model_id("claude-3-haiku".to_string()),
            ),
            ..Default::default()
        };
        assert_eq!(fixture, expected);
    }
}
