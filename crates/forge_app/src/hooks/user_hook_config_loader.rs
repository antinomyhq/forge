use std::path::Path;

use forge_domain::{UserHookConfig, UserSettings};
use tracing::{debug, warn};

/// Loads and merges user hook configurations from the three settings file
/// locations.
///
/// Resolution order (all merged, not overridden):
/// 1. `~/.forge/settings.json` (user-level, applies to all projects)
/// 2. `.forge/settings.json` (project-level, committable)
/// 3. `.forge/settings.local.json` (project-level, gitignored)
pub struct UserHookConfigLoader;

impl UserHookConfigLoader {
    /// Loads and merges hook configurations from all settings files.
    ///
    /// # Arguments
    /// * `home` - Home directory (e.g., `/Users/name`). If `None`, user-level
    ///   settings are skipped.
    /// * `cwd` - Current working directory for project-level settings.
    ///
    /// # Errors
    /// This function does not return errors. Invalid or missing files are
    /// silently skipped with a debug log.
    pub fn load(home: Option<&Path>, cwd: &Path) -> UserHookConfig {
        let mut config = UserHookConfig::new();

        // 1. User-level: ~/.forge/settings.json
        if let Some(home) = home {
            let user_settings_path = home.join("forge").join("settings.json");
            if let Some(user_config) = Self::load_file(&user_settings_path) {
                debug!(path = %user_settings_path.display(), "Loaded user-level hook config");
                config.merge(user_config);
            }
        }

        // 2. Project-level: .forge/settings.json
        let project_settings_path = cwd.join(".forge").join("settings.json");
        if let Some(project_config) = Self::load_file(&project_settings_path) {
            debug!(path = %project_settings_path.display(), "Loaded project-level hook config");
            config.merge(project_config);
        }

        // 3. Project-local: .forge/settings.local.json
        let local_settings_path = cwd.join(".forge").join("settings.local.json");
        if let Some(local_config) = Self::load_file(&local_settings_path) {
            debug!(path = %local_settings_path.display(), "Loaded project-local hook config");
            config.merge(local_config);
        }

        if !config.is_empty() {
            debug!(
                event_count = config.events.len(),
                "Merged user hook configuration"
            );
        }

        config
    }

    /// Loads a single settings file and extracts hook configuration.
    ///
    /// Returns `None` if the file doesn't exist or is invalid.
    fn load_file(path: &Path) -> Option<UserHookConfig> {
        let contents = match std::fs::read_to_string(path) {
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_load_nonexistent_paths() {
        let home = PathBuf::from("/nonexistent/home");
        let cwd = PathBuf::from("/nonexistent/project");

        let actual = UserHookConfigLoader::load(Some(&home), &cwd);
        assert!(actual.is_empty());
    }

    #[test]
    fn test_load_file_valid_settings() {
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

        let actual = UserHookConfigLoader::load_file(&settings_path);
        assert!(actual.is_some());
        let config = actual.unwrap();
        assert_eq!(
            config
                .get_groups(&forge_domain::UserHookEventName::PreToolUse)
                .len(),
            1
        );
    }

    #[test]
    fn test_load_file_settings_without_hooks() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        std::fs::write(&settings_path, r#"{"other_key": "value"}"#).unwrap();

        let actual = UserHookConfigLoader::load_file(&settings_path);
        assert!(actual.is_none());
    }

    #[test]
    fn test_load_merges_all_sources() {
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

        let actual = UserHookConfigLoader::load(Some(home_dir.path()), project_dir.path());

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
}
