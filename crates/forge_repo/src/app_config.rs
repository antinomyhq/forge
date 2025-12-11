use std::sync::Arc;

use anyhow::bail;
use bytes::Bytes;
use forge_app::{EnvironmentInfra, FileReaderInfra, FileWriterInfra};
use forge_domain::{AppConfig, AppConfigRepository, ModelId, ProviderId};
use tokio::sync::Mutex;

/// Repository for managing application configuration with caching support.
///
/// This repository uses infrastructure traits for file I/O operations and
/// maintains an in-memory cache to reduce file system access. The configuration
/// file path is automatically inferred from the environment.
pub struct AppConfigRepositoryImpl<F> {
    infra: Arc<F>,
    cache: Arc<Mutex<Option<AppConfig>>>,
}

impl<F> AppConfigRepositoryImpl<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra, cache: Arc::new(Mutex::new(None)) }
    }
}

impl<F: EnvironmentInfra + FileReaderInfra + FileWriterInfra> AppConfigRepositoryImpl<F> {
    async fn read_inner(&self) -> anyhow::Result<AppConfig> {
        let path = self.infra.get_environment().app_config();
        let content = self.infra.read_utf8(&path).await?;
        Ok(serde_json::from_str(&content)?)
    }

    async fn read(&self) -> AppConfig {
        self.read_inner().await.unwrap_or_default()
    }

    async fn write(&self, config: &AppConfig) -> anyhow::Result<()> {
        let path = self.infra.get_environment().app_config();
        let content = serde_json::to_string_pretty(config)?;
        self.infra.write(&path, Bytes::from(content)).await?;
        Ok(())
    }

    fn get_overrides(&self) -> (Option<ModelId>, Option<ProviderId>) {
        let env = self.infra.get_environment();
        (
            env.override_model.clone().map(ModelId::new),
            env.override_provider.clone().map(ProviderId::from),
        )
    }

    fn apply_overrides(&self, mut config: AppConfig) -> AppConfig {
        let (model, provider) = self.get_overrides();

        // Override the default provider first
        if let Some(ref provider_id) = provider {
            config.provider = Some(provider_id.clone());

            // If we have both provider and model overrides, ensure the model is set for
            // this provider
            if let Some(ref model_id) = model {
                config.model.insert(provider_id.clone(), model_id.clone());
            }
        }

        // If only model override (no provider override), update existing provider
        // models
        if provider.is_none() {
            if let Some(model_id) = model {
                if config.model.is_empty() {
                    // If no models configured but we have a default provider, set the model for it
                    if let Some(ref default_provider) = config.provider {
                        config.model.insert(default_provider.clone(), model_id);
                    }
                } else {
                    // Update all existing provider models
                    for (_, mut_model_id) in config.model.iter_mut() {
                        *mut_model_id = model_id.clone();
                    }
                }
            }
        }

        config
    }
}

#[async_trait::async_trait]
impl<F: EnvironmentInfra + FileReaderInfra + FileWriterInfra + Send + Sync> AppConfigRepository
    for AppConfigRepositoryImpl<F>
{
    async fn get_app_config(&self) -> anyhow::Result<AppConfig> {
        // Check cache first
        let cache = self.cache.lock().await;
        if let Some(ref cached_config) = *cache {
            // Apply overrides even to cached config since overrides can change via env vars
            return Ok(self.apply_overrides(cached_config.clone()));
        }
        drop(cache);

        // Cache miss, read from file
        let config = self.read().await;

        // Update cache with the newly read config (without overrides)
        let mut cache = self.cache.lock().await;
        *cache = Some(config.clone());

        // Apply overrides to the config before returning
        Ok(self.apply_overrides(config))
    }

    async fn set_app_config(&self, config: &AppConfig) -> anyhow::Result<()> {
        let (model, provider) = self.get_overrides();

        if model.is_some() || provider.is_some() {
            bail!("Could not save configuration: Model or Provider was overridden")
        }

        self.write(config).await?;

        // Bust the cache after successful write
        let mut cache = self.cache.lock().await;
        *cache = None;

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use std::collections::{BTreeMap, HashMap};
    use std::path::{Path, PathBuf};
    use std::str::FromStr;
    use std::sync::Mutex;

    use bytes::Bytes;
    use forge_app::{EnvironmentInfra, FileReaderInfra, FileWriterInfra};
    use forge_domain::{AppConfig, Config, ProviderId};
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use super::*;

    /// Mock infrastructure for testing that stores files in memory
    #[derive(Clone, derive_setters::Setters)]
    struct MockInfra {
        files: Arc<Mutex<HashMap<PathBuf, String>>>,
        config_path: PathBuf,
        #[setters(strip_option)]
        override_provider: Option<ProviderId>,
        #[setters(strip_option)]
        override_model: Option<ModelId>,
    }

    impl MockInfra {
        fn new(config_path: PathBuf) -> Self {
            Self {
                files: Arc::new(Mutex::new(HashMap::new())),
                config_path,
                override_provider: None,
                override_model: None,
            }
        }
    }

    impl EnvironmentInfra for MockInfra {
        fn get_environment(&self) -> Config {
            use fake::{Fake, Faker};
            let mut env: Config = Faker.fake();
            env = env.base_path(self.config_path.parent().unwrap().to_path_buf());

            if let Some(ref provider) = self.override_provider {
                env = env.override_provider(provider.clone());
            }
            if let Some(ref model) = self.override_model {
                env = env.override_model(model.clone());
            }

            env
        }

        fn get_env_var(&self, _key: &str) -> Option<String> {
            None
        }

        fn get_env_vars(&self) -> BTreeMap<String, String> {
            BTreeMap::new()
        }
    }

    #[async_trait::async_trait]
    impl FileReaderInfra for MockInfra {
        async fn read_utf8(&self, path: &Path) -> anyhow::Result<String> {
            self.files
                .lock()
                .unwrap()
                .get(path)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("File not found"))
        }

        async fn read(&self, _path: &Path) -> anyhow::Result<Vec<u8>> {
            unimplemented!()
        }

        async fn range_read_utf8(
            &self,
            _path: &Path,
            _start_line: u64,
            _end_line: u64,
        ) -> anyhow::Result<(String, forge_domain::FileInfo)> {
            unimplemented!()
        }
    }

    #[async_trait::async_trait]
    impl FileWriterInfra for MockInfra {
        async fn write(&self, path: &Path, contents: Bytes) -> anyhow::Result<()> {
            let content = String::from_utf8(contents.to_vec())?;
            self.files
                .lock()
                .unwrap()
                .insert(path.to_path_buf(), content);
            Ok(())
        }

        async fn write_temp(&self, _: &str, _: &str, _: &str) -> anyhow::Result<PathBuf> {
            unimplemented!()
        }
    }

    fn repository_fixture() -> (AppConfigRepositoryImpl<MockInfra>, TempDir) {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join(".config.json");
        let infra = Arc::new(MockInfra::new(config_path));
        (AppConfigRepositoryImpl::new(infra), temp_dir)
    }

    fn repository_with_config_fixture() -> (AppConfigRepositoryImpl<MockInfra>, TempDir) {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join(".config.json");

        // Create a config file with default config
        let config = AppConfig::default();
        let content = serde_json::to_string_pretty(&config).unwrap();

        let infra = Arc::new(MockInfra::new(config_path.clone()));
        infra.files.lock().unwrap().insert(config_path, content);

        (AppConfigRepositoryImpl::new(infra), temp_dir)
    }

    #[tokio::test]
    async fn test_get_app_config_exists() {
        let expected = AppConfig::default();
        let (repo, _temp_dir) = repository_with_config_fixture();

        let actual = repo.get_app_config().await.unwrap();

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_get_app_config_not_exists() {
        let (repo, _temp_dir) = repository_fixture();

        let actual = repo.get_app_config().await.unwrap();

        // Should return default config when file doesn't exist
        let expected = AppConfig::default();
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_set_app_config() {
        let fixture = AppConfig::default();
        let (repo, _temp_dir) = repository_fixture();

        let actual = repo.set_app_config(&fixture).await;

        assert!(actual.is_ok());

        // Verify the config was actually written by reading it back
        let read_config = repo.get_app_config().await.unwrap();
        assert_eq!(read_config, fixture);
    }

    #[tokio::test]
    async fn test_cache_behavior() {
        let (repo, _temp_dir) = repository_with_config_fixture();

        // First read should populate cache
        let first_read = repo.get_app_config().await.unwrap();

        // Second read should use cache (no file system access)
        let second_read = repo.get_app_config().await.unwrap();
        assert_eq!(first_read, second_read);

        // Write new config should bust cache
        let new_config = AppConfig::default();
        repo.set_app_config(&new_config).await.unwrap();

        // Next read should get fresh data
        let third_read = repo.get_app_config().await.unwrap();
        assert_eq!(third_read, new_config);
    }

    #[tokio::test]
    async fn test_read_handles_custom_provider() {
        let fixture = r#"{
            "provider": "xyz",
            "model": {}
        }"#;
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join(".config.json");

        let infra = Arc::new(MockInfra::new(config_path.clone()));
        infra
            .files
            .lock()
            .unwrap()
            .insert(config_path, fixture.to_string());

        let repo = AppConfigRepositoryImpl::new(infra);

        let actual = repo.get_app_config().await.unwrap();

        let expected = AppConfig {
            provider: Some(ProviderId::from_str("xyz").unwrap()),
            ..Default::default()
        };
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_read_returns_default_if_not_exists() {
        let (repo, _temp_dir) = repository_fixture();

        let config = repo.get_app_config().await.unwrap();

        // Config should be the default
        assert_eq!(config, AppConfig::default());
    }

    #[tokio::test]
    async fn test_override_model() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join(".config.json");

        // Set up a config with a specific model
        let mut config = AppConfig::default();
        config.model.insert(
            ProviderId::ANTHROPIC,
            ModelId::new("claude-3-5-sonnet-20241022"),
        );
        let content = serde_json::to_string_pretty(&config).unwrap();

        let infra = Arc::new(
            MockInfra::new(config_path.clone()).override_model(ModelId::new("override-model")),
        );
        infra.files.lock().unwrap().insert(config_path, content);

        let repo = AppConfigRepositoryImpl::new(infra);
        let actual = repo.get_app_config().await.unwrap();

        // The override model should be applied to all providers
        assert_eq!(
            actual.model.get(&ProviderId::ANTHROPIC),
            Some(&ModelId::new("override-model"))
        );
    }

    #[tokio::test]
    async fn test_override_provider() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join(".config.json");

        // Set up a config with a specific provider
        let config = AppConfig { provider: Some(ProviderId::ANTHROPIC), ..Default::default() };
        let content = serde_json::to_string_pretty(&config).unwrap();

        let infra =
            Arc::new(MockInfra::new(config_path.clone()).override_provider(ProviderId::OPENAI));
        infra.files.lock().unwrap().insert(config_path, content);

        let repo = AppConfigRepositoryImpl::new(infra);
        let actual = repo.get_app_config().await.unwrap();

        // The override provider should be applied
        assert_eq!(actual.provider, Some(ProviderId::OPENAI));
    }

    #[tokio::test]
    async fn test_override_prevents_config_write() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join(".config.json");

        let infra =
            Arc::new(MockInfra::new(config_path).override_model(ModelId::new("override-model")));
        let repo = AppConfigRepositoryImpl::new(infra);

        // Attempting to write config when override is set should fail
        let config = AppConfig::default();
        let actual = repo.set_app_config(&config).await;

        assert!(actual.is_err());
        assert!(actual
            .unwrap_err()
            .to_string()
            .contains("Model or Provider was overridden"));
    }

    #[tokio::test]
    async fn test_provider_override_applied_with_no_config() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join(".config.json");
        let expected = ProviderId::from_str("open_router").unwrap();

        let infra = Arc::new(
            MockInfra::new(config_path)
                .override_provider(expected.clone())
                .override_model(ModelId::new("test-model")),
        );
        let repo = AppConfigRepositoryImpl::new(infra);

        let actual = repo.get_app_config().await.unwrap();

        assert_eq!(actual.provider, Some(expected));
    }

    #[tokio::test]
    async fn test_model_override_applied_with_no_config() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join(".config.json");
        let provider = ProviderId::OPENAI;
        let expected = ModelId::new("gpt-4-test");

        let infra = Arc::new(
            MockInfra::new(config_path)
                .override_provider(provider.clone())
                .override_model(expected.clone()),
        );
        let repo = AppConfigRepositoryImpl::new(infra);

        let actual = repo.get_app_config().await.unwrap();

        assert_eq!(actual.model.get(&provider), Some(&expected));
    }

    #[tokio::test]
    async fn test_provider_override_on_cached_config() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join(".config.json");
        let expected = ProviderId::ANTHROPIC;

        let infra = Arc::new(
            MockInfra::new(config_path)
                .override_provider(expected.clone())
                .override_model(ModelId::new("test-model")),
        );
        let repo = AppConfigRepositoryImpl::new(infra);

        // First call populates cache
        repo.get_app_config().await.unwrap();

        // Second call should still apply override to cached config
        let actual = repo.get_app_config().await.unwrap();

        assert_eq!(actual.provider, Some(expected));
    }

    #[tokio::test]
    async fn test_model_override_on_cached_config() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join(".config.json");
        let provider = ProviderId::OPENAI;
        let expected = ModelId::new("gpt-4-cached");

        let infra = Arc::new(
            MockInfra::new(config_path)
                .override_provider(provider.clone())
                .override_model(expected.clone()),
        );
        let repo = AppConfigRepositoryImpl::new(infra);

        // First call populates cache
        repo.get_app_config().await.unwrap();

        // Second call should still apply override to cached config
        let actual = repo.get_app_config().await.unwrap();

        assert_eq!(actual.model.get(&provider), Some(&expected));
    }

    #[tokio::test]
    async fn test_model_override_with_existing_provider() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join(".config.json");
        let expected = ModelId::new("override-model");

        // Set up config with provider but no model
        let config = AppConfig { provider: Some(ProviderId::ANTHROPIC), ..Default::default() };
        let content = serde_json::to_string_pretty(&config).unwrap();

        let infra = Arc::new(MockInfra::new(config_path.clone()).override_model(expected.clone()));
        infra.files.lock().unwrap().insert(config_path, content);

        let repo = AppConfigRepositoryImpl::new(infra);
        let actual = repo.get_app_config().await.unwrap();

        assert_eq!(actual.model.get(&ProviderId::ANTHROPIC), Some(&expected));
    }
}
