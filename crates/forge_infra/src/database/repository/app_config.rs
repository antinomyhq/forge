use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context as _;
use forge_app::dto::AppConfig;
use forge_fs::ForgeFS;
use forge_services::AppConfigRepository;
use tokio::sync::Mutex;

pub struct AppConfigRepositoryImpl {
    pub config_path: PathBuf,
    cache: Arc<Mutex<Option<AppConfig>>>,
}

impl AppConfigRepositoryImpl {
    pub fn new(config_path: PathBuf) -> Self {
        Self { config_path, cache: Arc::new(Mutex::new(None)) }
    }

    async fn read(&self) -> anyhow::Result<AppConfig> {
        // Check if file exists, if not create with default config
        if !ForgeFS::exists(&self.config_path) {
            let default_config = AppConfig::default();
            let content = serde_json::to_string_pretty(&default_config)?;
            ForgeFS::write(&self.config_path, content).await?;
            return Ok(default_config);
        }

        let path = &self.config_path;
        let content = ForgeFS::read_utf8(&path)
            .await
            .with_context(|| format!("Failed to read app config: {}", path.display()))?;
        Ok(serde_json::from_str(&content)?)
    }

    async fn write(&self, config: &AppConfig) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(config)?;
        ForgeFS::write(&self.config_path, content).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl AppConfigRepository for AppConfigRepositoryImpl {
    async fn get_app_config(&self) -> anyhow::Result<AppConfig> {
        // Check cache first
        let cache = self.cache.lock().await;
        if let Some(ref cached_config) = *cache {
            return Ok(cached_config.clone());
        }
        drop(cache);

        // Cache miss, read from file
        let config = self.read().await?;

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

    use forge_app::dto::AppConfig;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use super::*;

    fn repository_fixture() -> anyhow::Result<(AppConfigRepositoryImpl, TempDir)> {
        let temp_dir = tempfile::tempdir()?;
        let config_path = temp_dir.path().join(".config.json");
        Ok((AppConfigRepositoryImpl::new(config_path), temp_dir))
    }

    fn repository_with_config_fixture() -> anyhow::Result<(AppConfigRepositoryImpl, TempDir)> {
        let temp_dir = tempfile::tempdir()?;
        let config_path = temp_dir.path().join(".config.json");

        // Create a config file with default config
        let config = AppConfig::default();
        let content = serde_json::to_string_pretty(&config)?;
        std::fs::write(&config_path, content)?;

        Ok((AppConfigRepositoryImpl::new(config_path), temp_dir))
    }

    #[tokio::test]
    async fn test_get_app_config_exists() -> anyhow::Result<()> {
        let expected = AppConfig::default();
        let (repo, _temp_dir) = repository_with_config_fixture()?;

        let actual = repo.get_app_config().await?;

        assert_eq!(actual, expected);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_app_config_not_exists() -> anyhow::Result<()> {
        let (repo, _temp_dir) = repository_fixture()?;

        let actual = repo.get_app_config().await?;

        // Should return default config when file doesn't exist
        let expected = AppConfig::default();
        assert_eq!(actual, expected);
        Ok(())
    }

    #[tokio::test]
    async fn test_set_app_config() -> anyhow::Result<()> {
        let fixture = AppConfig::default();
        let (repo, _temp_dir) = repository_fixture()?;

        let actual = repo.set_app_config(&fixture).await;

        assert!(actual.is_ok());

        // Verify the config was actually written by reading it back
        let read_config = repo.get_app_config().await?;
        assert_eq!(read_config, fixture);
        Ok(())
    }

    #[tokio::test]
    async fn test_cache_behavior() -> anyhow::Result<()> {
        let (repo, _temp_dir) = repository_with_config_fixture()?;

        // First read should populate cache
        let first_read = repo.get_app_config().await?;

        // Second read should use cache (no file system access)
        let second_read = repo.get_app_config().await?;
        assert_eq!(first_read, second_read);

        // Write new config should bust cache
        let new_config = AppConfig::default();
        repo.set_app_config(&new_config).await?;

        // Next read should get fresh data
        let third_read = repo.get_app_config().await?;
        assert_eq!(third_read, new_config);

        Ok(())
    }

    #[tokio::test]
    async fn test_read_creates_file_if_not_exists() -> anyhow::Result<()> {
        let (repo, _temp_dir) = repository_fixture()?;

        // File should not exist initially
        assert!(!repo.config_path.exists());

        // Reading should create the file with default config
        let config = repo.get_app_config().await?;

        // File should now exist
        assert!(repo.config_path.exists());

        // Config should be the default
        assert_eq!(config, AppConfig::default());

        Ok(())
    }
}
