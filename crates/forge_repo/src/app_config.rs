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
        self.write(config).await?;

        // Bust the cache after successful write
        let mut cache = self.cache.lock().await;
        *cache = None;

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    use bytes::Bytes;
    use forge_app::{EnvironmentInfra, FileReaderInfra, FileWriterInfra};
    use forge_domain::{AppConfig, Environment};
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use super::*;

    /// Mock infrastructure for testing that stores files in memory
    #[derive(Clone)]
    struct MockInfra {
        files: Arc<Mutex<HashMap<PathBuf, String>>>,
        config_path: PathBuf,
    }

    impl MockInfra {
        fn new(config_path: PathBuf) -> Self {
            Self { files: Arc::new(Mutex::new(HashMap::new())), config_path }
        }
    }

    impl EnvironmentInfra for MockInfra {
        fn get_environment(&self) -> Environment {
            use fake::{Fake, Faker};
            let env: Environment = Faker.fake();
            env.base_path(self.config_path.parent().unwrap().to_path_buf())
        }

        fn get_env_var(&self, _key: &str) -> Option<String> {
            None
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
    async fn test_read_handles_invalid_provider_gracefully() {
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

        let expected = AppConfig::default();
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_read_returns_default_if_not_exists() {
        let (repo, _temp_dir) = repository_fixture();

        let config = repo.get_app_config().await.unwrap();

        // Config should be the default
        assert_eq!(config, AppConfig::default());
    }
}
