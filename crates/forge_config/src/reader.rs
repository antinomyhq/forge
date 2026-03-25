use std::collections::HashMap;
use std::path::{Path, PathBuf};

use config::Config;
use serde::Deserialize;
use tracing::debug;

use crate::{ForgeConfig, ModelConfig};

/// Reads and merges [`ForgeConfig`] from multiple sources: embedded defaults,
/// home directory file, current working directory file, and environment
/// variables.
pub struct ConfigReader {}

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
    /// Creates a new `ConfigReader`.
    pub fn new() -> Self {
        Self {}
    }

    /// Returns the path to the legacy JSON config file: `~/forge/.config.json`.
    fn legacy_config_path() -> Option<PathBuf> {
        dirs::home_dir().map(|home| home.join("forge").join(".config.json"))
    }

    /// Reads and merges configuration from all sources, returning the resolved
    /// [`ForgeConfig`].
    ///
    /// Sources are applied in increasing priority order: embedded defaults,
    /// `~/forge/.config.json` (legacy JSON config, skipped when absent), the
    /// optional file at `path` (skipped when `None`), then environment
    /// variables prefixed with `FORGE_`.
    pub async fn read(&self, path: Option<&Path>) -> crate::Result<ForgeConfig> {
        let defaults = include_str!("../.forge.toml");
        let mut builder = Config::builder();

        // Load default
        builder = builder.add_source(config::File::from_str(defaults, config::FileFormat::Toml));

        // Load from ~/forge/.config.json (legacy format)
        if let Some(path) = Self::legacy_config_path() {
            if tokio::fs::try_exists(&path).await? {
                let json_contents = tokio::fs::read_to_string(&path).await?;
                if let Ok(json_config) = serde_json::from_str::<LegacyConfig>(&json_contents) {
                    let config = json_config.into_forge_config();
                    let toml_contents = toml_edit::ser::to_string(&config).unwrap_or_default();
                    builder = builder.add_source(config::File::from_str(
                        &toml_contents,
                        config::FileFormat::Toml,
                    ));
                }
            } else {
                debug!("Legacy config file not found at {:?}, skipping", path);
            }
        }

        // Load from path
        if let Some(path) = path
            && tokio::fs::try_exists(path).await?
        {
            let contents = tokio::fs::read_to_string(path).await?;
            builder =
                builder.add_source(config::File::from_str(&contents, config::FileFormat::Toml));
        }

        // Load from environment
        builder = builder.add_source(
            config::Environment::with_prefix("FORGE")
                .prefix_separator("_")
                .separator("__")
                .try_parsing(true)
                .list_separator(",")
                .with_list_parse_key("retry.status_codes")
                .with_list_parse_key("http.root_cert_paths"),
        );

        let config = builder.build()?;
        Ok(config.try_deserialize()?)
    }

    /// Reads and merges configuration from the embedded defaults and the given
    /// TOML string, returning the resolved [`ForgeConfig`].
    ///
    /// Unlike [`read`], this method accepts already-loaded TOML content and
    /// does not touch the filesystem or environment variables. This is
    /// appropriate when the caller has already read the raw file content via
    /// its own I/O abstraction.
    pub fn read_str(&self, contents: &str) -> crate::Result<ForgeConfig> {
        let defaults = include_str!("../.forge.toml");
        let config = Config::builder()
            .add_source(config::File::from_str(defaults, config::FileFormat::Toml))
            .add_source(config::File::from_str(contents, config::FileFormat::Toml))
            .build()?;
        Ok(config.try_deserialize()?)
    }

    /// Returns the [`ForgeConfig`] built from the embedded defaults only,
    /// without reading any file or environment variables.
    pub fn read_defaults(&self) -> ForgeConfig {
        let defaults = include_str!("../.forge.toml");
        Config::builder()
            .add_source(config::File::from_str(defaults, config::FileFormat::Toml))
            .build()
            .and_then(|c| c.try_deserialize())
            .expect("embedded .forge.toml defaults must always be valid")
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

    #[tokio::test]
    async fn test_read_parses_without_error() {
        let actual = ConfigReader::new().read(None).await;
        assert!(actual.is_ok(), "read() failed: {:?}", actual.err());
    }

    #[tokio::test]
    async fn test_read_session_from_env_vars() {
        let _ = EnvGuard::set(&[
            ("FORGE_SESSION__PROVIDER_ID", "fake-provider"),
            ("FORGE_SESSION__MODEL_ID", "fake-model"),
        ]);

        let actual = ConfigReader::new().read(None).await.unwrap();

        let expected = Some(ModelConfig {
            provider_id: Some("fake-provider".to_string()),
            model_id: Some("fake-model".to_string()),
        });
        assert_eq!(actual.session, expected);
    }
}
