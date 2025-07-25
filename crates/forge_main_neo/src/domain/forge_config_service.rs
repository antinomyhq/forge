use std::path::Path;

use anyhow::Result;

use crate::domain::forge_config::ForgeConfig;

/// Service for loading and managing forge configuration
pub struct ForgeConfigService;

impl ForgeConfigService {
    /// Load forge configuration from the default location (forge.yaml)
    pub async fn load_config() -> Result<ForgeConfig> {
        Self::load_config_from_path("forge.yaml").await
    }

    /// Load forge configuration from a specific path
    pub async fn load_config_from_path(path: impl AsRef<Path>) -> Result<ForgeConfig> {
        let path = path.as_ref();

        if !path.exists() {
            // Return empty config if file doesn't exist
            return Ok(ForgeConfig::default());
        }

        let content = tokio::fs::read_to_string(path).await?;
        ForgeConfig::from_yaml(&content)
    }

    /// Check if forge.yaml exists in the current directory
    #[cfg(test)]
    pub fn config_exists() -> bool {
        Path::new("forge.yaml").exists()
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tempfile::NamedTempFile;
    use tokio::fs::write;

    use super::*;

    #[tokio::test]
    async fn test_load_config_from_path_success() {
        let temp_file = NamedTempFile::new().unwrap();
        let config_content = r#"
commands:
- name: test
  description: Test command
  prompt: Test prompt
model: test-model
"#;
        write(temp_file.path(), config_content).await.unwrap();

        let actual = ForgeConfigService::load_config_from_path(temp_file.path())
            .await
            .unwrap();
        let expected = ForgeConfig::from_yaml(config_content).unwrap();
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_load_config_from_path_not_exists() {
        let actual = ForgeConfigService::load_config_from_path("nonexistent.yaml")
            .await
            .unwrap();
        let expected = ForgeConfig::default();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_config_exists_false() {
        let actual = ForgeConfigService::config_exists();
        let expected = false; // Assuming forge.yaml doesn't exist in test environment
        assert_eq!(actual, expected);
    }
}
