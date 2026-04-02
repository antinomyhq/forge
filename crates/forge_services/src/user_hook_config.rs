use std::path::Path;
use std::sync::Arc;

use forge_app::{EnvironmentInfra, FileInfoInfra, FileReaderInfra};
use forge_domain::{UserHookConfig, UserSettings};

/// Loads and merges user hook configurations from the three settings file
/// locations using infrastructure abstractions.
///
/// Resolution order (all merged, not overridden):
/// 1. `~/.forge/settings.json` (user-level, applies to all projects)
/// 2. `.forge/settings.json` (project-level, committable)
/// 3. `.forge/settings.local.json` (project-level, gitignored)
pub struct ForgeUserHookConfigService<F>(Arc<F>);

impl<F> ForgeUserHookConfigService<F> {
    /// Creates a new service with the given infrastructure dependency.
    pub fn new(infra: Arc<F>) -> Self {
        Self(infra)
    }
}

impl<F: FileInfoInfra + FileReaderInfra + EnvironmentInfra> ForgeUserHookConfigService<F> {
    /// Loads a single settings file and extracts hook configuration.
    ///
    /// Returns `Ok(None)` if the file does not exist or cannot be read.
    /// Returns `Err` if the file exists but fails to deserialize, including the file path in the
    /// error message.
    async fn load_file(&self, path: &Path) -> anyhow::Result<Option<UserHookConfig>> {
        if !self.0.exists(path).await? {
            return Ok(None);
        }
        let contents = self
            .0
            .read_utf8(path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", path.display(), e))?;

        match serde_json::from_str::<UserSettings>(&contents) {
            Ok(settings) => {
                if settings.hooks.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(settings.hooks))
                }
            }
            Err(e) => Err(anyhow::anyhow!(
                "Failed to deserialize '{}': {}",
                path.display(),
                e
            )),
        }
    }
}

#[async_trait::async_trait]
impl<F: FileInfoInfra + FileReaderInfra + EnvironmentInfra> forge_app::UserHookConfigService
    for ForgeUserHookConfigService<F>
{
    async fn get_user_hook_config(&self) -> anyhow::Result<UserHookConfig> {
        let env = self.0.get_environment();

        // Collect all candidate paths in resolution order
        let mut paths: Vec<std::path::PathBuf> = Vec::new();
        if let Some(home) = &env.home {
            paths.push(home.join("forge").join("settings.json"));
        }
        paths.push(env.cwd.join(".forge").join("settings.json"));
        paths.push(env.cwd.join(".forge").join("settings.local.json"));

        // Load every file, keeping the (path, result) pairs
        let results =
            futures::future::join_all(paths.iter().map(|path| self.load_file(path))).await;

        // Collect the error message from every file that failed
        let errors: Vec<String> = results
            .iter()
            .filter_map(|r| r.as_ref().err().map(|e| e.to_string()))
            .collect();

        if !errors.is_empty() {
            return Err(anyhow::anyhow!("{}", errors.join("\n\n")));
        }

        // Merge every successfully loaded config
        let mut config = UserHookConfig::new();
        for result in results {
            if let Ok(Some(file_config)) = result {
                config.merge(file_config);
            }
        }

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use fake::Fake;
    use forge_app::UserHookConfigService;
    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn test_load_file_valid_settings() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        std::fs::write(
            &settings_path,
            r#"{
                "hooks": {
                    "PreToolUse": [
                        { "matcher": "Bash", "hooks": [{ "type": "command", "command": "check.sh" }] }
                    ]
                }
            }"#,
        )
            .unwrap();

        let service = fixture(None, PathBuf::from("/nonexistent"));

        let actual = service.load_file(&settings_path).await.unwrap();
        assert!(actual.is_some());
        let config = actual.unwrap();
        assert_eq!(
            config
                .get_groups(&forge_domain::UserHookEventName::PreToolUse)
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn test_load_file_settings_without_hooks() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        std::fs::write(&settings_path, r#"{"other_key": "value"}"#).unwrap();

        let service = fixture(None, PathBuf::from("/nonexistent"));

        let actual = service.load_file(&settings_path).await.unwrap();
        assert!(actual.is_none());
    }

    #[tokio::test]
    async fn test_load_file_invalid_json_returns_error_with_path() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        std::fs::write(&settings_path, r#"{ invalid json }"#).unwrap();

        let service = fixture(None, PathBuf::from("/nonexistent"));

        let actual = service.load_file(&settings_path).await;
        assert!(actual.is_err());
        let err = actual.unwrap_err().to_string();
        assert!(
            err.contains(&settings_path.display().to_string()),
            "Error message should contain the file path, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_get_user_hook_config_reports_all_invalid_files() {
        let project_dir = tempfile::tempdir().unwrap();
        let project_forge_dir = project_dir.path().join(".forge");
        std::fs::create_dir_all(&project_forge_dir).unwrap();

        // Both project files have invalid JSON
        std::fs::write(project_forge_dir.join("settings.json"), r#"{ bad }"#).unwrap();
        std::fs::write(
            project_forge_dir.join("settings.local.json"),
            r#"{ also bad }"#,
        )
        .unwrap();

        let service = fixture(None, project_dir.path().to_path_buf());

        let actual = service.get_user_hook_config().await;
        assert!(actual.is_err());
        let err = actual.unwrap_err().to_string();
        assert!(
            err.contains("settings.json"),
            "Error should mention settings.json, got: {err}"
        );
        assert!(
            err.contains("settings.local.json"),
            "Error should mention settings.local.json, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_get_user_hook_config_nonexistent_paths() {
        let service = fixture(
            Some(PathBuf::from("/nonexistent/home")),
            PathBuf::from("/nonexistent/project"),
        );

        let actual = service.get_user_hook_config().await.unwrap();
        assert!(actual.is_empty());
    }

    #[tokio::test]
    async fn test_get_user_hook_config_merges_all_sources() {
        // Set up a fake home directory
        let home_dir = tempfile::tempdir().unwrap();
        let forge_dir = home_dir.path().join("forge");
        std::fs::create_dir_all(&forge_dir).unwrap();
        std::fs::write(
            forge_dir.join("settings.json"),
            r#"{
                "hooks": {
                    "PreToolUse": [
                        { "matcher": "Bash", "hooks": [{ "type": "command", "command": "global.sh" }] }
                    ]
                }
            }"#,
        )
            .unwrap();

        // Set up a project directory
        let project_dir = tempfile::tempdir().unwrap();
        let project_forge_dir = project_dir.path().join(".forge");
        std::fs::create_dir_all(&project_forge_dir).unwrap();
        std::fs::write(
            project_forge_dir.join("settings.json"),
            r#"{
                "hooks": {
                    "PreToolUse": [
                        { "matcher": "Write", "hooks": [{ "type": "command", "command": "project.sh" }] }
                    ]
                }
            }"#,
        )
            .unwrap();
        std::fs::write(
            project_forge_dir.join("settings.local.json"),
            r#"{
                "hooks": {
                    "Stop": [
                        { "hooks": [{ "type": "command", "command": "local-stop.sh" }] }
                    ]
                }
            }"#,
        )
        .unwrap();

        let service = fixture(
            Some(home_dir.path().to_path_buf()),
            project_dir.path().to_path_buf(),
        );

        let actual = service.get_user_hook_config().await.unwrap();

        // PreToolUse should have 2 groups (global + project)
        assert_eq!(
            actual
                .get_groups(&forge_domain::UserHookEventName::PreToolUse)
                .len(),
            2
        );
        // Stop should have 1 group (local)
        assert_eq!(
            actual
                .get_groups(&forge_domain::UserHookEventName::Stop)
                .len(),
            1
        );
    }

    // --- Test helpers ---

    fn fixture(home: Option<PathBuf>, cwd: PathBuf) -> ForgeUserHookConfigService<TestInfra> {
        ForgeUserHookConfigService::new(Arc::new(TestInfra { home, cwd }))
    }

    struct TestInfra {
        home: Option<PathBuf>,
        cwd: PathBuf,
    }

    #[async_trait::async_trait]
    impl FileInfoInfra for TestInfra {
        async fn is_binary(&self, _path: &Path) -> anyhow::Result<bool> {
            Ok(false)
        }

        async fn is_file(&self, path: &Path) -> anyhow::Result<bool> {
            Ok(tokio::fs::metadata(path)
                .await
                .map(|m| m.is_file())
                .unwrap_or(false))
        }

        async fn exists(&self, path: &Path) -> anyhow::Result<bool> {
            Ok(tokio::fs::try_exists(path).await.unwrap_or(false))
        }

        async fn file_size(&self, path: &Path) -> anyhow::Result<u64> {
            Ok(tokio::fs::metadata(path).await?.len())
        }
    }

    #[async_trait::async_trait]
    impl FileReaderInfra for TestInfra {
        async fn read_utf8(&self, path: &Path) -> anyhow::Result<String> {
            Ok(tokio::fs::read_to_string(path).await?)
        }

        fn read_batch_utf8(
            &self,
            _batch_size: usize,
            _paths: Vec<PathBuf>,
        ) -> impl futures::Stream<Item = (PathBuf, anyhow::Result<String>)> + Send {
            futures::stream::empty()
        }

        async fn read(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
            Ok(tokio::fs::read(path).await?)
        }

        async fn range_read_utf8(
            &self,
            _path: &Path,
            _start_line: u64,
            _end_line: u64,
        ) -> anyhow::Result<(String, forge_domain::FileInfo)> {
            unimplemented!("not needed for tests")
        }
    }

    impl EnvironmentInfra for TestInfra {
        type Config = forge_config::ForgeConfig;

        fn get_env_var(&self, _key: &str) -> Option<String> {
            None
        }

        fn get_env_vars(&self) -> std::collections::BTreeMap<String, String> {
            Default::default()
        }

        fn get_environment(&self) -> forge_domain::Environment {
            let mut env: forge_domain::Environment = fake::Faker.fake();
            env.home = self.home.clone();
            env.cwd = self.cwd.clone();
            env
        }

        fn get_config(&self) -> forge_config::ForgeConfig {
            Default::default()
        }

        async fn update_environment(
            &self,
            _ops: Vec<forge_domain::ConfigOperation>,
        ) -> anyhow::Result<()> {
            unimplemented!("not needed for tests")
        }
    }
}
