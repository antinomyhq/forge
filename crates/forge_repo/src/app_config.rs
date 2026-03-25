use std::sync::Arc;

use forge_config::{ForgeConfig, ModelConfig};
use forge_domain::{
    AppConfig, AppConfigRepository, CommitConfig, LoginInfo, ModelId, ProviderId, SuggestConfig,
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

/// Overlays the [`AppConfig`] fields onto an existing [`ForgeConfig`],
/// preserving all other fields (retry, http, limits, etc.).
fn app_config_to_forge_config(app: &AppConfig, mut fc: ForgeConfig) -> ForgeConfig {
    // Login info — flattened into top-level fields
    match &app.key_info {
        Some(info) => {
            fc.api_key = Some(info.api_key.clone());
            fc.api_key_name = Some(info.api_key_name.clone());
            fc.api_key_masked = Some(info.api_key_masked.clone());
            fc.email = info.email.clone();
            fc.name = info.name.clone();
            fc.auth_provider_id = info.auth_provider_id.clone();
        }
        None => {
            fc.api_key = None;
            fc.api_key_name = None;
            fc.api_key_masked = None;
            fc.email = None;
            fc.name = None;
            fc.auth_provider_id = None;
        }
    }

    // Active model — use the provider's entry from the model map
    fc.session = app.provider.as_ref().map(|pid| {
        let mut config = ModelConfig::default().provider_id(pid.as_ref().to_string());
        if let Some(mid) = app.model.get(pid) {
            config = config.model_id(mid.to_string());
        }

        config
    });

    fc.commit = app.commit.as_ref().and_then(|cc| {
        cc.provider
            .as_ref()
            .zip(cc.model.as_ref())
            .map(|(pid, mid)| {
                ModelConfig::default()
                    .provider_id(pid.as_ref().to_string())
                    .model_id(mid.to_string())
            })
    });

    fc.suggest = app.suggest.as_ref().map(|sc| {
        ModelConfig::default()
            .provider_id(sc.provider.as_ref().to_string())
            .model_id(sc.model.to_string())
    });

    fc
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
        let config = ForgeConfig::read().await;

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

    /// Writes [`AppConfig`] to disk via [`ForgeConfig::write`], preserving all
    /// non-`AppConfig` fields from the existing file.
    async fn write(&self, config: &AppConfig) -> anyhow::Result<ForgeConfig> {
        debug!(config = ?config, "writing app-config");
        let existing = ForgeConfig::read().await.unwrap_or_else(|e| {
            tracing::warn!(error = %e, "Could not read existing config; defaults will be used.");
            forge_config::ConfigReader::new().read_defaults()
        });
        let config = app_config_to_forge_config(config, existing);

        config.write().await?;
        debug!(config = ?config, "written .forge.toml");
        Ok(config)
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

    async fn set_app_config(&self, config: &AppConfig) -> anyhow::Result<()> {
        let config = self.write(config).await?;

        // Bust the cache after successful write
        let mut cache = self.cache.lock().await;
        *cache = Some(config);

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::str::FromStr;
    use std::sync::Mutex;

    use forge_domain::ProviderId;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use super::*;

    /// Mutex to serialize all tests that mutate the `HOME` env var, preventing
    /// races when multiple tests run concurrently in the same process.
    static HOME_MUTEX: Mutex<()> = Mutex::new(());

    /// Guard type that holds both the mutex guard and the temp dir, ensuring
    /// the temp directory outlives the mutex release.
    struct HomeGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        _dir: TempDir,
    }

    /// Sets HOME to a fresh temp directory so that [`ForgeConfig::read`] and
    /// [`ForgeConfig::write`] operate on an isolated `~/.forge/.forge.toml`.
    /// Acquires the [`HOME_MUTEX`] and holds it for the lifetime of the
    /// returned guard.
    fn temp_home() -> HomeGuard {
        let lock = HOME_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: tests are serialized by HOME_MUTEX, so no concurrent HOME reads
        // occur.
        unsafe { std::env::set_var("HOME", dir.path()) };
        HomeGuard { _lock: lock, _dir: dir }
    }

    impl std::ops::Deref for HomeGuard {
        type Target = TempDir;
        fn deref(&self) -> &TempDir {
            &self._dir
        }
    }

    /// Returns the path to `.forge.toml` inside a temp home directory.
    fn forge_toml_path(home: &HomeGuard) -> PathBuf {
        home.path().join(".forge").join(".forge.toml")
    }

    /// Writes a TOML string to the forge config path, creating parent dirs.
    fn write_toml(home: &HomeGuard, toml: &str) {
        let path = forge_toml_path(home);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, toml).unwrap();
    }

    fn repository_fixture(_home: &HomeGuard) -> ForgeConfigRepository {
        ForgeConfigRepository::new()
    }

    /// Returns a [`ForgeConfig`] built from embedded defaults only, as a
    /// clean starting point for conversion fixtures.
    fn forge_config_defaults() -> ForgeConfig {
        forge_config::ConfigReader::new().read_defaults()
    }

    // -------------------------------------------------------------------------
    // forge_config_to_app_config
    // -------------------------------------------------------------------------

    #[test]
    fn test_forge_config_to_app_config_empty() {
        let fixture = forge_config_defaults();

        let actual = forge_config_to_app_config(fixture);

        let expected = AppConfig::default();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_forge_config_to_app_config_with_model() {
        let mut fixture = forge_config_defaults();
        fixture.session = Some(
            ModelConfig::default()
                .provider_id("anthropic".to_string())
                .model_id("claude-3-5-sonnet-20241022".to_string()),
        );

        let actual = forge_config_to_app_config(fixture);

        let expected = AppConfig {
            provider: Some(ProviderId::ANTHROPIC),
            model: HashMap::from([(
                ProviderId::ANTHROPIC,
                ModelId::new("claude-3-5-sonnet-20241022"),
            )]),
            ..Default::default()
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_forge_config_to_app_config_with_login_info() {
        let mut fixture = forge_config_defaults();
        fixture.api_key = Some("sk-test-key".to_string());
        fixture.api_key_name = Some("my-key".to_string());
        fixture.api_key_masked = Some("sk-***".to_string());
        fixture.email = Some("user@example.com".to_string());
        fixture.name = Some("Alice".to_string());
        fixture.auth_provider_id = Some("github".to_string());

        let actual = forge_config_to_app_config(fixture);

        let expected = AppConfig {
            key_info: Some(LoginInfo {
                api_key: "sk-test-key".to_string(),
                api_key_name: "my-key".to_string(),
                api_key_masked: "sk-***".to_string(),
                email: Some("user@example.com".to_string()),
                name: Some("Alice".to_string()),
                auth_provider_id: Some("github".to_string()),
            }),
            ..Default::default()
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_forge_config_to_app_config_no_login_info_when_api_key_absent() {
        let fixture = forge_config_defaults();
        // api_key is None → key_info must be None even if other fields are set

        let actual = forge_config_to_app_config(fixture);

        assert_eq!(actual.key_info, None);
    }

    #[test]
    fn test_forge_config_to_app_config_with_commit() {
        let mut fixture = forge_config_defaults();
        fixture.commit = Some(
            ModelConfig::default()
                .provider_id("openai".to_string())
                .model_id("gpt-4o".to_string()),
        );

        let actual = forge_config_to_app_config(fixture);

        let expected = CommitConfig {
            provider: Some(ProviderId::OPENAI),
            model: Some(ModelId::new("gpt-4o")),
        };
        assert_eq!(actual.commit, Some(expected));
    }

    #[test]
    fn test_forge_config_to_app_config_with_suggest() {
        let mut fixture = forge_config_defaults();
        fixture.suggest = Some(
            ModelConfig::default()
                .provider_id("openai".to_string())
                .model_id("gpt-4o-mini".to_string()),
        );

        let actual = forge_config_to_app_config(fixture);

        let expected = SuggestConfig {
            provider: ProviderId::OPENAI,
            model: ModelId::new("gpt-4o-mini"),
        };
        assert_eq!(actual.suggest, Some(expected));
    }

    #[test]
    fn test_forge_config_to_app_config_session_provider_only() {
        let mut fixture = forge_config_defaults();
        fixture.session =
            Some(ModelConfig::default().provider_id("anthropic".to_string()));

        let actual = forge_config_to_app_config(fixture);

        assert_eq!(actual.provider, Some(ProviderId::ANTHROPIC));
        assert!(actual.model.is_empty());
    }

    #[test]
    fn test_forge_config_to_app_config_session_model_only() {
        let mut fixture = forge_config_defaults();
        fixture.session = Some(ModelConfig::default().model_id("claude-3-5-sonnet-20241022".to_string()));

        let actual = forge_config_to_app_config(fixture);

        assert_eq!(actual.provider, None);
        assert!(actual.model.is_empty());
    }

    // -------------------------------------------------------------------------
    // app_config_to_forge_config
    // -------------------------------------------------------------------------

    #[test]
    fn test_app_config_to_forge_config_empty() {
        let app = AppConfig::default();
        let base = forge_config_defaults();

        let actual = app_config_to_forge_config(&app, base.clone());

        // All AppConfig-owned fields must be cleared; unrelated fields preserved.
        assert_eq!(actual.api_key, None);
        assert_eq!(actual.session, None);
        assert_eq!(actual.commit, None);
        assert_eq!(actual.suggest, None);
        // Non-AppConfig field is unchanged
        assert_eq!(actual.retry, base.retry);
    }

    #[test]
    fn test_app_config_to_forge_config_with_model() {
        let app = AppConfig {
            provider: Some(ProviderId::ANTHROPIC),
            model: HashMap::from([(
                ProviderId::ANTHROPIC,
                ModelId::new("claude-3-5-sonnet-20241022"),
            )]),
            ..Default::default()
        };
        let base = forge_config_defaults();

        let actual = app_config_to_forge_config(&app, base);

        let expected_model = ModelConfig::default()
            .provider_id("anthropic".to_string())
            .model_id("claude-3-5-sonnet-20241022".to_string());
        assert_eq!(actual.session, Some(expected_model));
    }

    #[test]
    fn test_app_config_to_forge_config_model_uses_active_provider() {
        // Two providers in the map; only the active provider's model is written.
        let app = AppConfig {
            provider: Some(ProviderId::OPENAI),
            model: HashMap::from([
                (ProviderId::OPENAI, ModelId::new("gpt-4o")),
                (
                    ProviderId::ANTHROPIC,
                    ModelId::new("claude-3-5-sonnet-20241022"),
                ),
            ]),
            ..Default::default()
        };
        let base = forge_config_defaults();

        let actual = app_config_to_forge_config(&app, base);

        let expected = ModelConfig::default()
            .provider_id("openai".to_string())
            .model_id("gpt-4o".to_string());
        assert_eq!(actual.session, Some(expected));
    }

    #[test]
    fn test_app_config_to_forge_config_no_model_when_provider_missing() {
        // Model map has an entry but no active provider → model field is None.
        let app = AppConfig {
            provider: None,
            model: HashMap::from([(ProviderId::OPENAI, ModelId::new("gpt-4o"))]),
            ..Default::default()
        };
        let base = forge_config_defaults();

        let actual = app_config_to_forge_config(&app, base);

        assert_eq!(actual.session, None);
    }

    #[test]
    fn test_app_config_to_forge_config_provider_only_no_model_in_map() {
        // Provider is set but model map has no entry for it → session has provider_id but no model_id.
        let app = AppConfig {
            provider: Some(ProviderId::ANTHROPIC),
            model: HashMap::new(),
            ..Default::default()
        };
        let base = forge_config_defaults();

        let actual = app_config_to_forge_config(&app, base);

        let expected = ModelConfig::default().provider_id("anthropic".to_string());
        assert_eq!(actual.session, Some(expected));
    }

    #[test]
    fn test_app_config_to_forge_config_with_login_info() {
        let app = AppConfig {
            key_info: Some(LoginInfo {
                api_key: "sk-test-key".to_string(),
                api_key_name: "my-key".to_string(),
                api_key_masked: "sk-***".to_string(),
                email: Some("user@example.com".to_string()),
                name: Some("Alice".to_string()),
                auth_provider_id: Some("github".to_string()),
            }),
            ..Default::default()
        };
        let base = forge_config_defaults();

        let actual = app_config_to_forge_config(&app, base);

        assert_eq!(actual.api_key, Some("sk-test-key".to_string()));
        assert_eq!(actual.api_key_name, Some("my-key".to_string()));
        assert_eq!(actual.api_key_masked, Some("sk-***".to_string()));
        assert_eq!(actual.email, Some("user@example.com".to_string()));
        assert_eq!(actual.name, Some("Alice".to_string()));
        assert_eq!(actual.auth_provider_id, Some("github".to_string()));
    }

    #[test]
    fn test_app_config_to_forge_config_clears_login_info_when_absent() {
        // Start with a base that has login fields set, then overlay an AppConfig
        // with no key_info — all login fields must be cleared.
        let mut base = forge_config_defaults();
        base.api_key = Some("old-key".to_string());
        base.email = Some("old@example.com".to_string());

        let app = AppConfig::default(); // key_info is None

        let actual = app_config_to_forge_config(&app, base);

        assert_eq!(actual.api_key, None);
        assert_eq!(actual.email, None);
    }

    #[test]
    fn test_app_config_to_forge_config_preserves_unrelated_fields() {
        // Non-AppConfig fields (retry, http, limits, …) must survive a round-trip.
        let app = AppConfig::default();
        let base = forge_config_defaults();
        let expected_retry = base.retry.clone();
        let expected_max_search = base.max_search_lines;

        let actual = app_config_to_forge_config(&app, base);

        assert_eq!(actual.retry, expected_retry);
        assert_eq!(actual.max_search_lines, expected_max_search);
    }

    #[test]
    fn test_round_trip_forge_config_to_app_config_and_back() {
        let mut original = forge_config_defaults();
        original.api_key = Some("sk-test".to_string());
        original.api_key_name = Some("test-key".to_string());
        original.api_key_masked = Some("sk-***".to_string());
        original.session = Some(
            ModelConfig::default()
                .provider_id("anthropic".to_string())
                .model_id("claude-3-5-sonnet-20241022".to_string()),
        );
        original.commit = Some(
            ModelConfig::default()
                .provider_id("openai".to_string())
                .model_id("gpt-4o".to_string()),
        );

        let app = forge_config_to_app_config(original.clone());
        let actual = app_config_to_forge_config(&app, original.clone());

        assert_eq!(actual.api_key, original.api_key);
        assert_eq!(actual.api_key_name, original.api_key_name);
        assert_eq!(actual.session, original.session);
        assert_eq!(actual.commit, original.commit);
    }

    #[tokio::test]
    async fn test_get_app_config_not_exists() {
        let _home = temp_home();
        let repo = repository_fixture(&_home);

        let actual = repo.get_app_config().await.unwrap();

        assert_eq!(actual, forge_domain::AppConfig::default());
    }

    #[tokio::test]
    async fn test_set_app_config() {
        let _home = temp_home();
        let fixture = forge_domain::AppConfig::default();
        let repo = repository_fixture(&_home);

        let actual = repo.set_app_config(&fixture).await;

        assert!(actual.is_ok());

        // Verify the config was actually written by reading it back
        let read_config = repo.get_app_config().await.unwrap();
        assert_eq!(read_config, fixture);
    }

    #[tokio::test]
    async fn test_cache_behavior() {
        let _home = temp_home();
        write_toml(&_home, "");
        let repo = repository_fixture(&_home);

        // First read should populate cache
        let first_read = repo.get_app_config().await.unwrap();

        // Second read should use cache (no file system access)
        let second_read = repo.get_app_config().await.unwrap();
        assert_eq!(first_read, second_read);

        // Write new config should bust cache
        let new_config = forge_domain::AppConfig::default();
        repo.set_app_config(&new_config).await.unwrap();

        // Next read should get fresh data
        let third_read = repo.get_app_config().await.unwrap();
        assert_eq!(third_read, new_config);
    }

    #[test]
    fn test_read_handles_custom_provider() {
        // Verify the full parse path for a custom provider value — uses
        // ConfigReader::read_str to avoid any real filesystem dependency.
        let toml = r#"
[session]
provider_id = "xyz"
model_id = "some-model"
"#;
        let fc = forge_config::ConfigReader::new().read_str(toml).unwrap();

        let actual = forge_config_to_app_config(fc);

        assert_eq!(actual.provider, Some(ProviderId::from_str("xyz").unwrap()));
    }
}
