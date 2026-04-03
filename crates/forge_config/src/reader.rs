use std::path::{Path, PathBuf};
use std::sync::{LazyLock, OnceLock};

use config::ConfigBuilder;
use config::builder::DefaultState;

use crate::ForgeConfig;
use crate::legacy::LegacyConfig;

/// Loads all `.env` files found while walking up from the current working
/// directory to the root, with priority given to closer (lower) directories.
/// Executed at most once per process.
static LOAD_DOT_ENV: LazyLock<()> = LazyLock::new(|| {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut paths = vec![];
    let mut current = PathBuf::new();

    for component in cwd.components() {
        current.push(component);
        paths.push(current.clone());
    }

    paths.reverse();

    for path in paths {
        let env_file = path.join(".env");
        if env_file.is_file() {
            dotenvy::from_path(&env_file).ok();
        }
    }
});

static MIGRATE_CONFIG_PATHS: OnceLock<()> = OnceLock::new();

fn old_base_path() -> PathBuf {
    dirs::home_dir().unwrap_or(PathBuf::from(".")).join("forge")
}

fn new_base_path() -> PathBuf {
    dirs::home_dir().unwrap_or(PathBuf::from(".")).join(".forge")
}

fn migrate_file(old_path: &Path, new_path: &Path) -> std::io::Result<()> {
    if new_path.exists() || !old_path.exists() {
        return Ok(());
    }

    if let Some(parent) = new_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::rename(old_path, new_path)?;
    Ok(())
}

fn migrate_base_dir(old_base: &Path, new_base: &Path) -> std::io::Result<()> {
    if new_base.exists() || !old_base.exists() {
        return Ok(());
    }

    std::fs::rename(old_base, new_base)?;
    Ok(())
}

fn migrate_paths() {
    MIGRATE_CONFIG_PATHS.get_or_init(|| {
        let old_base = old_base_path();
        let new_base = new_base_path();

        let _ = migrate_base_dir(&old_base, &new_base);
        let _ = migrate_file(&new_base.join(".forge.toml"), &new_base.join("config.toml"));
    });
}

/// Merges [`ForgeConfig`] from layered sources using a builder pattern.
#[derive(Default)]
pub struct ConfigReader {
    builder: ConfigBuilder<DefaultState>,
}

impl ConfigReader {
    /// Returns the path to the legacy JSON config file
    /// (`~/.forge/.config.json`).
    pub fn config_legacy_path() -> PathBuf {
        migrate_paths();
        Self::base_path().join(".config.json")
    }

    /// Returns the path to the primary TOML config file
    /// (`~/.forge/config.toml`).
    pub fn config_path() -> PathBuf {
        migrate_paths();
        Self::base_path().join("config.toml")
    }

    /// Returns the base directory for all Forge config files (`~/.forge`).
    pub fn base_path() -> PathBuf {
        migrate_paths();
        new_base_path()
    }

    /// Adds the provided TOML string as a config source without touching the
    /// filesystem.
    pub fn read_toml(mut self, contents: &str) -> Self {
        self.builder = self
            .builder
            .add_source(config::File::from_str(contents, config::FileFormat::Toml));

        self
    }

    /// Adds the embedded default config (`../config.toml`) as a source.
    pub fn read_defaults(self) -> Self {
        let defaults = include_str!("../config.toml");

        self.read_toml(defaults)
    }

    /// Adds `FORGE_`-prefixed environment variables as a config source.
    pub fn read_env(mut self) -> Self {
        self.builder = self.builder.add_source(
            config::Environment::with_prefix("FORGE")
                .prefix_separator("_")
                .separator("__")
                .try_parsing(true)
                .list_separator(",")
                .with_list_parse_key("retry.status_codes")
                .with_list_parse_key("http.root_cert_paths"),
        );

        self
    }

    /// Builds and deserializes all accumulated sources into a [`ForgeConfig`].
    ///
    /// Triggers `.env` file loading (at most once per process) by walking up
    /// the directory tree from the current working directory, with closer
    /// directories taking priority.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration cannot be built or deserialized.
    pub fn build(self) -> crate::Result<ForgeConfig> {
        *LOAD_DOT_ENV;
        let config = self.builder.build()?;
        Ok(config.try_deserialize::<ForgeConfig>()?)
    }

    /// Adds `~/.forge/config.toml` as a config source, silently skipping if
    /// absent.
    pub fn read_global(mut self) -> Self {
        let path = Self::config_path();
        self.builder = self
            .builder
            .add_source(config::File::from(path).required(false));
        self
    }

    /// Reads `~/.forge/.config.json` (legacy format) and adds it as a source,
    /// silently skipping errors.
    pub fn read_legacy(self) -> Self {
        let content = LegacyConfig::read(&Self::config_legacy_path());
        if let Ok(content) = content {
            self.read_toml(&content)
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::{Mutex, MutexGuard};
    use std::time::{SystemTime, UNIX_EPOCH};

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::ModelConfig;

    /// Serializes tests that mutate environment variables to prevent races.
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    /// Holds env vars set for a test's duration and removes them on drop, while
    /// holding [`ENV_MUTEX`].
    struct EnvGuard {
        keys: Vec<&'static str>,
        _lock: MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        /// Sets each `(key, value)` pair in the environment, returning a guard
        /// that cleans them up on drop.
        #[must_use]
        fn set(pairs: &[(&'static str, &str)]) -> Self {
            let lock = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
            let keys = pairs.iter().map(|(k, _)| *k).collect();
            for (key, value) in pairs {
                unsafe { std::env::set_var(key, value) };
            }
            Self { keys, _lock: lock }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for key in &self.keys {
                unsafe { std::env::remove_var(key) };
            }
        }
    }

    #[test]
    fn test_read_parses_without_error() {
        let actual = ConfigReader::default().read_defaults().build();
        assert!(actual.is_ok(), "read() failed: {:?}", actual.err());
    }

    fn temp_fixture_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("forge-config-{name}-{nonce}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_migrate_base_dir_moves_legacy_forge_directory() {
        let fixture = temp_fixture_dir("base-dir");
        let old_base = fixture.join("forge");
        let new_base = fixture.join(".forge");
        fs::create_dir_all(&old_base).unwrap();
        fs::write(old_base.join("marker.txt"), "migrated").unwrap();

        let actual = migrate_base_dir(&old_base, &new_base);
        actual.unwrap();
        let expected = "migrated";

        assert!(!old_base.exists());
        assert_eq!(fs::read_to_string(new_base.join("marker.txt")).unwrap(), expected);
        fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn test_migrate_file_renames_dot_forge_toml_to_config_toml() {
        let fixture = temp_fixture_dir("config-file");
        let old_path = fixture.join(".forge.toml");
        let new_path = fixture.join("config.toml");
        fs::write(&old_path, "key = 'value'").unwrap();

        let actual = migrate_file(&old_path, &new_path);
        actual.unwrap();
        let expected = "key = 'value'";

        assert!(!old_path.exists());
        assert_eq!(fs::read_to_string(new_path).unwrap(), expected);
        fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn test_legacy_layer_does_not_overwrite_defaults() {
        // Simulate what `read_legacy` does: serialize a ForgeConfig that only
        // carries session/commit/suggest (all other fields are None) and layer
        // it on top of the embedded defaults. The default values must survive.
        let legacy = ForgeConfig {
            session: Some(ModelConfig {
                provider_id: Some("anthropic".to_string()),
                model_id: Some("claude-3".to_string()),
            }),
            ..Default::default()
        };
        let legacy_toml = toml_edit::ser::to_string_pretty(&legacy).unwrap();

        let actual = ConfigReader::default()
            // Read legacy first and then defaults
            .read_toml(&legacy_toml)
            .read_defaults()
            .build()
            .unwrap();

        // Session should come from the legacy layer
        assert_eq!(
            actual.session,
            Some(ModelConfig {
                provider_id: Some("anthropic".to_string()),
                model_id: Some("claude-3".to_string()),
            })
        );

        // Default values from config.toml must be retained, not reset to zero
        assert_eq!(actual.max_parallel_file_reads, 64);
        assert_eq!(actual.max_read_lines, 2000);
        assert_eq!(actual.tool_timeout_secs, 300);
        assert_eq!(actual.max_search_lines, 1000);
        assert_eq!(actual.tool_supported, true);
    }

    #[test]
    fn test_read_session_from_env_vars() {
        let _guard = EnvGuard::set(&[
            ("FORGE_SESSION__PROVIDER_ID", "fake-provider"),
            ("FORGE_SESSION__MODEL_ID", "fake-model"),
        ]);

        let actual = ConfigReader::default()
            .read_defaults()
            .read_env()
            .build()
            .unwrap();

        let expected = Some(ModelConfig {
            provider_id: Some("fake-provider".to_string()),
            model_id: Some("fake-model".to_string()),
        });
        assert_eq!(actual.session, expected);
    }
}
