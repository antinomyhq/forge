use std::path::Path;
use std::sync::Arc;

use forge_app::{EnvironmentInfra, FileReaderInfra};
use forge_domain::{UserHookConfig, UserSettings};
use tracing::{debug, warn};

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

impl<F: FileReaderInfra + EnvironmentInfra> ForgeUserHookConfigService<F> {
    /// Loads a single settings file and extracts hook configuration.
    ///
    /// Returns `None` if the file doesn't exist or is invalid.
    async fn load_file(&self, path: &Path) -> Option<UserHookConfig> {
        let contents = match self.0.read_utf8(path).await {
            Ok(c) => c,
            Err(_) => return None,
        };

        match serde_json::from_str::<UserSettings>(&contents) {
            Ok(settings) => {
                if settings.hooks.is_empty() {
                    None
                } else {
                    Some(settings.hooks)
                }
            }
            Err(e) => {
                warn!(
                    path = %path.display(),
                    error = %e,
                    "Failed to parse settings file for hooks"
                );
                None
            }
        }
    }
}

#[async_trait::async_trait]
impl<F: FileReaderInfra + EnvironmentInfra> forge_app::UserHookConfigService
    for ForgeUserHookConfigService<F>
{
    async fn get_user_hook_config(&self) -> anyhow::Result<UserHookConfig> {
        let env = self.0.get_environment();
        let mut config = UserHookConfig::new();

        // 1. User-level: ~/.forge/settings.json
        if let Some(home) = &env.home {
            let user_settings_path = home.join("forge").join("settings.json");
            if let Some(user_config) = self.load_file(&user_settings_path).await {
                debug!(path = %user_settings_path.display(), "Loaded user-level hook config");
                config.merge(user_config);
            }
        }

        // 2. Project-level: .forge/settings.json
        let project_settings_path = env.cwd.join(".forge").join("settings.json");
        if let Some(project_config) = self.load_file(&project_settings_path).await {
            debug!(path = %project_settings_path.display(), "Loaded project-level hook config");
            config.merge(project_config);
        }

        // 3. Project-local: .forge/settings.local.json
        let local_settings_path = env.cwd.join(".forge").join("settings.local.json");
        if let Some(local_config) = self.load_file(&local_settings_path).await {
            debug!(path = %local_settings_path.display(), "Loaded project-local hook config");
            config.merge(local_config);
        }

        if !config.is_empty() {
            debug!(
                event_count = config.events.len(),
                "Merged user hook configuration"
            );
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

        let actual = service.load_file(&settings_path).await;
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

        let actual = service.load_file(&settings_path).await;
        assert!(actual.is_none());
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

        async fn update_environment(
            &self,
            _ops: Vec<forge_domain::ConfigOperation>,
        ) -> anyhow::Result<()> {
            unimplemented!("not needed for tests")
        }
    }
}
