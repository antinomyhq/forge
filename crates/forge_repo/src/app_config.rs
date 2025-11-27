use std::sync::Arc;

use bytes::Bytes;
use forge_app::{EnvironmentInfra, FileReaderInfra, FileWriterInfra};
use forge_domain::{AppConfig, AppConfigRepository};
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

impl<F: EnvironmentInfra + FileReaderInfra> AppConfigRepositoryImpl<F> {
    async fn read_inner(&self) -> anyhow::Result<AppConfig> {
        let path = self.infra.get_environment().app_config();
        let content = self.infra.read_utf8(&path).await?;
        Ok(serde_json::from_str(&content)?)
    }

    async fn read(&self) -> AppConfig {
        self.read_inner().await.unwrap_or_default()
    }
}

impl<F: EnvironmentInfra + FileWriterInfra> AppConfigRepositoryImpl<F> {
    async fn write(&self, config: &AppConfig) -> anyhow::Result<()> {
        let path = self.infra.get_environment().app_config();
        let content = serde_json::to_string_pretty(config)?;
        self.infra.write(&path, Bytes::from(content)).await?;
        Ok(())
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
            return Ok(cached_config.clone());
        }
        drop(cache);

        // Cache miss, read from file
        let config = self.read().await;

        // Update cache with the newly read config
        let mut cache = self.cache.lock().await;
        *cache = Some(config.clone());

        Ok(config)
    }

    async fn set_app_config(&self, config: &AppConfig) -> anyhow::Result<()> {
        // Check if ephemeral mode is enabled via environment variable
        let ephemeral = self
            .infra
            .get_env_var("FORGE_EPHEMERAL_APP_CONFIG")
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(false);

        if ephemeral {
            // Ephemeral mode: only update cache, don't write to disk
            let mut cache = self.cache.lock().await;
            *cache = Some(config.clone());
        } else {
            // Normal mode: write to disk and bust cache
            self.write(config).await?;

            // Bust the cache after successful write
            let mut cache = self.cache.lock().await;
            *cache = None;
        }

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
    use forge_domain::{AppConfig, Environment, ProviderId};
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use super::*;

    /// Mock infrastructure for testing that stores files in memory
    #[derive(Clone)]
    struct MockInfra {
        files: Arc<Mutex<HashMap<PathBuf, String>>>,
        config_path: PathBuf,
        env_vars: Arc<Mutex<HashMap<String, String>>>,
    }

    impl MockInfra {
        fn new(config_path: PathBuf) -> Self {
            Self {
                files: Arc::new(Mutex::new(HashMap::new())),
                config_path,
                env_vars: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        fn with_env_var(self, key: &str, value: &str) -> Self {
            self.env_vars
                .lock()
                .unwrap()
                .insert(key.to_string(), value.to_string());
            self
        }
    }

    impl EnvironmentInfra for MockInfra {
        fn get_environment(&self) -> Environment {
            use fake::{Fake, Faker};
            let env: Environment = Faker.fake();
            env.base_path(self.config_path.parent().unwrap().to_path_buf())
        }

        fn get_env_var(&self, key: &str) -> Option<String> {
            self.env_vars.lock().unwrap().get(key).cloned()
        }

        fn get_env_vars(&self) -> BTreeMap<String, String> {
            self.env_vars
                .lock()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
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
        repository_fixture_with_env(HashMap::new())
    }

    fn repository_fixture_with_env(
        env_vars: HashMap<String, String>,
    ) -> (AppConfigRepositoryImpl<MockInfra>, TempDir) {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join(".config.json");
        let mut infra = MockInfra::new(config_path);

        // Set environment variables
        for (key, value) in env_vars {
            infra = infra.with_env_var(&key, &value);
        }

        (AppConfigRepositoryImpl::new(Arc::new(infra)), temp_dir)
    }

    fn repository_with_config_fixture() -> (AppConfigRepositoryImpl<MockInfra>, TempDir) {
        repository_with_config_and_env_fixture(HashMap::new())
    }

    fn repository_with_config_and_env_fixture(
        env_vars: HashMap<String, String>,
    ) -> (AppConfigRepositoryImpl<MockInfra>, TempDir) {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join(".config.json");

        // Create a config file with default config
        let config = AppConfig::default();
        let content = serde_json::to_string_pretty(&config).unwrap();

        let mut infra = MockInfra::new(config_path.clone());

        // Set environment variables
        for (key, value) in env_vars {
            infra = infra.with_env_var(&key, &value);
        }

        infra.files.lock().unwrap().insert(config_path, content);

        (AppConfigRepositoryImpl::new(Arc::new(infra)), temp_dir)
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
    async fn test_ephemeral_mode_only_updates_cache() {
        use forge_domain::{ModelId, ProviderId};

        // Create fixture with ephemeral mode enabled
        let env_vars = [("FORGE_EPHEMERAL_APP_CONFIG".to_string(), "true".to_string())]
            .into_iter()
            .collect();
        let (repo, _temp_dir) = repository_with_config_and_env_fixture(env_vars);

        // Create a modified config with runtime overrides
        let ephemeral_config = AppConfig {
            key_info: None,
            provider: Some(ProviderId::OPENAI),
            model: [(ProviderId::OPENAI, ModelId::new("gpt-4o"))]
                .into_iter()
                .collect(),
        };

        // Set app config in ephemeral mode (should only update cache)
        repo.set_app_config(&ephemeral_config).await.unwrap();

        // Get config should return the ephemeral config
        let actual = repo.get_app_config().await.unwrap();
        assert_eq!(actual, ephemeral_config);

        // Verify file was NOT written by reading directly
        let file_content = repo.infra.files.lock().unwrap();
        let file_config_str = file_content.get(&repo.infra.config_path).unwrap();
        let file_config: AppConfig = serde_json::from_str(file_config_str).unwrap();

        // File should still contain the default config, not the ephemeral config
        let expected = AppConfig::default();
        assert_eq!(file_config, expected);
    }

    #[tokio::test]
    async fn test_ephemeral_config_cache_used_before_disk() {
        use forge_domain::{ModelId, ProviderId};

        // Create fixture with ephemeral mode enabled
        let env_vars = [("FORGE_EPHEMERAL_APP_CONFIG".to_string(), "true".to_string())]
            .into_iter()
            .collect();
        let (repo, _temp_dir) = repository_with_config_and_env_fixture(env_vars);

        // Set ephemeral config
        let ephemeral_config = AppConfig {
            key_info: None,
            provider: Some(ProviderId::ANTHROPIC),
            model: [(
                ProviderId::ANTHROPIC,
                ModelId::new("claude-3-5-sonnet-20241022"),
            )]
            .into_iter()
            .collect(),
        };

        repo.set_app_config(&ephemeral_config).await.unwrap();

        // First read should use cache
        let first_read = repo.get_app_config().await.unwrap();
        assert_eq!(first_read, ephemeral_config);

        // Second read should also use cache
        let second_read = repo.get_app_config().await.unwrap();
        assert_eq!(second_read, ephemeral_config);
    }

    #[tokio::test]
    async fn test_normal_mode_writes_to_disk() {
        use forge_domain::ProviderId;

        // Create fixture without ephemeral mode (normal mode)
        let (repo, _temp_dir) = repository_with_config_fixture();

        // Set config in normal mode
        let normal_config = AppConfig { provider: Some(ProviderId::OPENAI), ..Default::default() };
        repo.set_app_config(&normal_config).await.unwrap();

        // Verify config is written to disk
        {
            let file_content = repo.infra.files.lock().unwrap();
            let file_config_str = file_content.get(&repo.infra.config_path).unwrap();
            let file_config: AppConfig = serde_json::from_str(file_config_str).unwrap();
            assert_eq!(file_config.provider, Some(ProviderId::OPENAI));
        } // Drop the lock before the next write

        // Write new config to disk (should bust cache)
        let persistent_config =
            AppConfig { provider: Some(ProviderId::ANTHROPIC), ..Default::default() };
        repo.set_app_config(&persistent_config).await.unwrap();

        // Next read should return the persisted config, not the runtime cache
        let after_write = repo.get_app_config().await.unwrap();
        assert_eq!(after_write.provider, Some(ProviderId::ANTHROPIC));
    }
}
