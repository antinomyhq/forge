use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ModelConfig;

/// Intermediate representation of the legacy `~/forge/.config.json` format.
///
/// This format stores the active provider as a top-level string and models as
/// a map from provider ID to model ID, which differs from the TOML config's
/// nested `session`, `commit`, and `suggest` sub-objects.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyConfig {
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

/// Partial config containing only the fields that the legacy JSON format can
/// express. Serializing this struct produces a TOML fragment that will not
/// overwrite unrelated fields (e.g. `max_parallel_file_reads`) in the config
/// builder's merge.
#[derive(Serialize)]
struct LegacyPartialConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    session: Option<ModelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    commit: Option<ModelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggest: Option<ModelConfig>,
}

impl LegacyConfig {
    /// Reads the legacy `~/forge/.config.json` file at `path`, parses it, and
    /// returns the equivalent TOML representation as a [`String`].
    ///
    /// Only the fields that the legacy format covers (`session`, `commit`,
    /// `suggest`) are included in the output so that unrelated config values
    /// from lower-priority layers are not overwritten with zero-defaults.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read, the JSON is invalid, or the
    /// resulting config cannot be serialized to TOML.
    pub(crate) fn read(path: &PathBuf) -> crate::Result<String> {
        let contents = std::fs::read_to_string(path)?;
        let config = serde_json::from_str::<LegacyConfig>(&contents)?;
        let partial = config.into_partial_config();
        let content = toml_edit::ser::to_string_pretty(&partial)?;
        Ok(content)
    }

    /// Converts a [`LegacyConfig`] into only the fields it actually carries,
    /// avoiding the inclusion of zero-default values for unrelated fields.
    fn into_partial_config(self) -> LegacyPartialConfig {
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

        LegacyPartialConfig { session, commit, suggest }
    }
}
