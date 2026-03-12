use config::{ConfigBuilder, Environment, File, FileFormat, builder::AsyncState};
use serde::de::DeserializeOwned;

use crate::config::ForgeConfig;

/// Errors that can occur when reading configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigReadError {
    /// The configuration source failed to load or a value could not be deserialized.
    #[error(transparent)]
    Config(#[from] config::ConfigError),
}

/// Reads and deserializes a [`ForgeConfig`] from the following sources, in increasing priority
/// order:
///
/// 1. A YAML file at `{path}.yaml` (optional — skipped if the file does not exist)
/// 2. A JSON file at `{path}.json` (optional — skipped if the file does not exist)
/// 3. Environment variables (always active)
///
/// # Arguments
///
/// * `path` - Base file path without extension. The reader appends `.yaml` / `.json` automatically.
///
/// # Errors
///
/// Returns [`ConfigReadError`] if any source fails to parse or if deserialization into
/// [`ForgeConfig`] fails.
pub async fn read(path: &str) -> Result<ForgeConfig, ConfigReadError> {
    read_as::<ForgeConfig>(path).await
}

/// Reads and deserializes any `T: DeserializeOwned` from the following sources, in increasing
/// priority order:
///
/// 1. A YAML file at `{path}.yaml` (optional — skipped if the file does not exist)
/// 2. A JSON file at `{path}.json` (optional — skipped if the file does not exist)
/// 3. Environment variables (always active)
///
/// # Arguments
///
/// * `path` - Base file path without extension.
///
/// # Errors
///
/// Returns [`ConfigReadError`] if any source fails to parse or if deserialization into `T` fails.
pub async fn read_as<T: DeserializeOwned>(path: &str) -> Result<T, ConfigReadError> {
    let cfg = ConfigBuilder::<AsyncState>::default()
        .add_source(File::new(&format!("{path}.yaml"), FileFormat::Yaml).required(false))
        .add_source(File::new(&format!("{path}.json"), FileFormat::Json).required(false))
        .add_source(Environment::default())
        .build()
        .await?;

    let value = cfg.try_deserialize::<T>()?;
    Ok(value)
}
