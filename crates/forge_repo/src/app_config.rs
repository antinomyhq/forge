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
        let mut fc = ConfigReader::default().read_global().build()?;

        debug!(config = ?fc, "loaded config for update");

        // Apply each operation directly onto ForgeConfig
        debug!(?ops, "applying app config operations");
        for op in ops {
            apply_op(op, &mut fc);
        }

        // Persist
        fc.write().await?;
        debug!(config = ?fc, "written .forge.toml");

        // Reset cache
        let mut cache = self.cache.lock().await;
        *cache = None;

        Ok(())
    }
}
