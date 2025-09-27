use std::path::PathBuf;

use forge_app::dto::AppConfig;
use forge_fs::ForgeFS;
use forge_services::AppConfigRepository;

pub struct AppConfigRepositoryImpl {
    config_path: PathBuf,
}

impl AppConfigRepositoryImpl {
    pub fn new(config_path: PathBuf) -> Self {
        Self { config_path }
    }

    async fn read(&self) -> anyhow::Result<AppConfig> {
        let content = ForgeFS::read_utf8(&self.config_path).await?;
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
    async fn get_app_config(&self) -> anyhow::Result<Option<AppConfig>> {
        match self.read().await {
            Ok(config) => Ok(Some(config)),
            Err(_) => Ok(None),
        }
    }

    async fn set_app_config(&self, config: &AppConfig) -> anyhow::Result<()> {
        self.write(config).await
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
        let expected = Some(AppConfig::default());
        let (repo, _temp_dir) = repository_with_config_fixture()?;

        let actual = repo.get_app_config().await?;

        assert_eq!(actual, expected);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_app_config_not_exists() -> anyhow::Result<()> {
        let (repo, _temp_dir) = repository_fixture()?;

        let actual = repo.get_app_config().await?;

        assert_eq!(actual, None);
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
        assert_eq!(read_config, Some(fixture));
        Ok(())
    }
}
