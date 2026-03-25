use std::collections::HashMap;
use std::path::PathBuf;

use config::ConfigBuilder;
use config::builder::DefaultState;
use serde::Deserialize;
use tracing::debug;

use crate::{ForgeConfig, ModelConfig};

/// Reads and merges [`ForgeConfig`] from multiple sources: embedded defaults,
/// home directory file, current working directory file, and environment
/// variables.
#[derive(Default)]
pub struct ConfigReader {
    builder: ConfigBuilder<DefaultState>,
}

/// Intermediate representation of the legacy `~/forge/.config.json` format.
///
/// This format stores the active provider as a top-level string and models as
/// a map from provider ID to model ID, which differs from the TOML config's
/// nested `session`, `commit`, and `suggest` sub-objects.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyConfig {
    /// The active provider ID (e.g. `"anthropic"`).
    #[serde(default)]
    provider: Option<String>,
    /// Map from provider ID to the model ID to use with that provider.
    #[serde(default)]
    model: HashMap<String, String>,
    /// Commit message generation provider/model pair.
    #[serde(default)]
    commit: Option<LegacyModelRef>,
    /// Shell command suggestion provider/model pair.
    #[serde(default)]
    suggest: Option<LegacyModelRef>,
}

/// A provider/model pair as expressed in the legacy JSON config.
#[derive(Debug, Deserialize)]
struct LegacyModelRef {
    provider: Option<String>,
    model: Option<String>,
}

impl LegacyConfig {
    /// Converts a [`LegacyConfig`] into the fields of [`ForgeConfig`] that it
    /// covers, leaving all other fields at their defaults.
    fn into_forge_config(self) -> ForgeConfig {
        let session = self.provider.as_deref().map(|provider_id| {
            let model_id = self.model.get(provider_id).cloned();
            ModelConfig { provider_id: Some(provider_id.to_string()), model_id }
        });

        let commit = self
            .commit
            .map(|c| ModelConfig { provider_id: c.provider, model_id: c.model });

        let suggest = self
            .suggest
            .map(|s| ModelConfig { provider_id: s.provider, model_id: s.model });

        ForgeConfig { session, commit, suggest, ..Default::default() }
    }
}

impl ConfigReader {
    pub fn config_legacy_path() -> PathBuf {
        Self::base_path().join(".config.json")
    }

    pub fn config_path() -> PathBuf {
        Self::base_path().join(".forge.toml")
    }

    pub fn base_path() -> PathBuf {
        dirs::home_dir().unwrap_or(PathBuf::from(".")).join("forge")
    }

    /// Reads and merges configuration from the embedded defaults and the given
    /// TOML string, returning the resolved [`ForgeConfig`].
    ///
    /// Unlike [`read`], this method accepts already-loaded TOML content and
    /// does not touch the filesystem or environment variables. This is
    /// appropriate when the caller has already read the raw file content via
    /// its own I/O abstraction.
    pub fn read_toml(mut self, contents: &str) -> Self {
        self.builder = self
            .builder
            .add_source(config::File::from_str(contents, config::FileFormat::Toml));

        self
    }

    /// Returns the [`ForgeConfig`] built from the embedded defaults only,
    /// without reading any file or environment variables.
    pub fn read_defaults(self) -> Self {
        let defaults = include_str!("../.forge.toml");

        self.read_toml(defaults)
    }

    /// Adds environment variables prefixed with `FORGE_` as a source.
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

    /// Builds and returns the merged [`ForgeConfig`] from all accumulated sources.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration cannot be built or deserialized.
    pub fn build(self) -> crate::Result<ForgeConfig> {
        let config = self.builder.build()?;
        Ok(config.try_deserialize::<ForgeConfig>()?)
    }

    /// Reads `~/.forge/.forge.toml` and adds it as a config source.
    ///
    /// If the file does not exist it is silently skipped. If the file cannot
    /// be read or parsed the error is propagated.
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be read or deserialized.

    pub fn read_global(mut self) -> Self {
        let path = Self::config_path();
        self.builder = self.builder.add_source(config::File::from(path));
        self
    }

    /// Reads `~/.forge/.config.json` (the legacy JSON format), converts it to
    /// a [`ForgeConfig`], and adds it as a config source.
    ///
    /// If the file does not exist or cannot be parsed it is silently skipped.
    pub fn read_legacy(mut self) -> Self {
        let path = Self::config_legacy_path();
        self.builder = self.builder.add_source(config::File::from(path));

        self
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, MutexGuard};

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::ModelConfig;

    /// Serializes tests that mutate environment variables to prevent races.
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    /// Guard that holds a set of environment variables for the duration of a
    /// test, removing them all on drop. Also holds the [`ENV_MUTEX`] lock to
    /// prevent concurrent env mutations across tests.
    struct EnvGuard {
        keys: Vec<&'static str>,
        _lock: MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        /// Sets each `(key, value)` pair in the process environment and returns
        /// a guard that removes all those keys when dropped.
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
