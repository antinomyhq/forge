use std::sync::Arc;

use forge_config::{ConfigReader, ForgeConfig, ModelConfig};
use forge_domain::{
    AppConfig, AppConfigOperation, AppConfigRepository, CommitConfig, LoginInfo, ModelId,
    ProviderId, SuggestConfig,
};
use tokio::sync::Mutex;
use tracing::{debug, error};

/// Converts a [`ForgeConfig`] into an [`AppConfig`].
///
/// `ForgeConfig` flattens login info as top-level fields and represents the
/// active model as a single [`ModelConfig`]. This conversion reconstructs the
/// nested [`LoginInfo`] and per-provider model map used by the domain.
fn forge_config_to_app_config(fc: ForgeConfig) -> AppConfig {
    let key_info = fc.api_key.map(|api_key| LoginInfo {
        api_key,
        api_key_name: fc.api_key_name.unwrap_or_default(),
        api_key_masked: fc.api_key_masked.unwrap_or_default(),
        email: fc.email,
        name: fc.name,
        auth_provider_id: fc.auth_provider_id,
    });

    let (provider, model) = match fc.session {
        Some(mc) => {
            let provider_id = mc.provider_id.map(ProviderId::from);
            let mut map = std::collections::HashMap::new();
            if let (Some(ref pid), Some(mid)) = (provider_id.clone(), mc.model_id.map(ModelId::new))
            {
                map.insert(pid.clone(), mid);
            }
            (provider_id, map)
        }
        None => (None, std::collections::HashMap::new()),
    };

    let commit = fc.commit.map(|mc| CommitConfig {
        provider: mc.provider_id.map(ProviderId::from),
        model: mc.model_id.map(ModelId::new),
    });

    let suggest = fc.suggest.and_then(|mc| {
        mc.provider_id
            .zip(mc.model_id)
            .map(|(pid, mid)| SuggestConfig {
                provider: ProviderId::from(pid),
                model: ModelId::new(mid),
            })
    });

    AppConfig { key_info, provider, model, commit, suggest }
}

/// Applies a single [`AppConfigOperation`] directly onto a [`ForgeConfig`]
/// in-place, bypassing the intermediate [`AppConfig`] representation.
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

    /// Reads [`AppConfig`] from disk via [`ForgeConfig::read`].
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
    async fn get_app_config(&self) -> anyhow::Result<AppConfig> {
        // Check cache first
        let cache = self.cache.lock().await;
        if let Some(ref config) = *cache {
            return Ok(forge_config_to_app_config(config.clone()));
        }
        drop(cache);

        // Cache miss, read from file
        let config = self.read().await;

        let mut cache = self.cache.lock().await;
        *cache = Some(config.clone());

        Ok(forge_config_to_app_config(config))
    }

    async fn update_app_config(&self, ops: Vec<AppConfigOperation>) -> anyhow::Result<()> {
        // Load the global config
        let mut fc = ConfigReader::default()
            .read_defaults()
            .read_global()
            .build()?;

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
    use std::collections::HashMap;

    use forge_config::{ForgeConfig, ModelConfig};
    use forge_domain::{
        AppConfig, AppConfigOperation, CommitConfig, LoginInfo, ModelId, ProviderId, SuggestConfig,
    };
    use pretty_assertions::assert_eq;

    use super::{apply_op, forge_config_to_app_config};

    // ── forge_config_to_app_config ────────────────────────────────────────────

    #[test]
    fn test_empty_forge_config_produces_empty_app_config() {
        let fixture = ForgeConfig::default();
        let actual = forge_config_to_app_config(fixture);
        let expected = AppConfig::default();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_full_login_info_is_mapped() {
        let fixture = ForgeConfig::default()
            .api_key("key-abc".to_string())
            .api_key_name("My Key".to_string())
            .api_key_masked("key-***".to_string())
            .email("user@example.com".to_string())
            .name("Alice".to_string())
            .auth_provider_id("github".to_string());
        let actual = forge_config_to_app_config(fixture);
        let expected = AppConfig {
            key_info: Some(LoginInfo {
                api_key: "key-abc".to_string(),
                api_key_name: "My Key".to_string(),
                api_key_masked: "key-***".to_string(),
                email: Some("user@example.com".to_string()),
                name: Some("Alice".to_string()),
                auth_provider_id: Some("github".to_string()),
            }),
            ..Default::default()
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_session_with_provider_and_model() {
        let fixture = ForgeConfig {
            session: Some(
                ModelConfig::default()
                    .provider_id("anthropic".to_string())
                    .model_id("claude-3".to_string()),
            ),
            ..Default::default()
        };
        let actual = forge_config_to_app_config(fixture);
        let provider = ProviderId::from("anthropic".to_string());
        let expected = AppConfig {
            provider: Some(provider.clone()),
            model: HashMap::from([(provider, ModelId::new("claude-3"))]),
            ..Default::default()
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_session_with_only_provider_leaves_model_map_empty() {
        let fixture = ForgeConfig {
            session: Some(ModelConfig::default().provider_id("openai".to_string())),
            ..Default::default()
        };
        let actual = forge_config_to_app_config(fixture);
        let expected = AppConfig {
            provider: Some(ProviderId::from("openai".to_string())),
            model: HashMap::new(),
            ..Default::default()
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_commit_config_is_mapped() {
        let fixture = ForgeConfig {
            commit: Some(
                ModelConfig::default()
                    .provider_id("openai".to_string())
                    .model_id("gpt-4o".to_string()),
            ),
            ..Default::default()
        };
        let actual = forge_config_to_app_config(fixture);
        let expected = AppConfig {
            commit: Some(CommitConfig {
                provider: Some(ProviderId::from("openai".to_string())),
                model: Some(ModelId::new("gpt-4o")),
            }),
            ..Default::default()
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_suggest_config_requires_both_provider_and_model() {
        let fixture_provider_only = ForgeConfig {
            suggest: Some(ModelConfig::default().provider_id("openai".to_string())),
            ..Default::default()
        };
        assert_eq!(
            forge_config_to_app_config(fixture_provider_only).suggest,
            None
        );

        let fixture_model_only = ForgeConfig {
            suggest: Some(ModelConfig { model_id: Some("gpt-4o".to_string()), provider_id: None }),
            ..Default::default()
        };
        assert_eq!(forge_config_to_app_config(fixture_model_only).suggest, None);
    }

    #[test]
    fn test_suggest_config_with_both_fields_is_mapped() {
        let fixture = ForgeConfig {
            suggest: Some(
                ModelConfig::default()
                    .provider_id("openai".to_string())
                    .model_id("gpt-4o-mini".to_string()),
            ),
            ..Default::default()
        };
        let actual = forge_config_to_app_config(fixture);
        let expected = AppConfig {
            suggest: Some(SuggestConfig {
                provider: ProviderId::from("openai".to_string()),
                model: ModelId::new("gpt-4o-mini"),
            }),
            ..Default::default()
        };
        assert_eq!(actual, expected);
    }

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
