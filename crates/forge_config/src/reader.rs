use std::path::Path;

use config::Config;

use crate::ForgeConfig;

/// Reads and merges [`ForgeConfig`] from multiple sources: embedded defaults,
/// home directory file, current working directory file, and environment
/// variables.
pub struct ConfigReader {}

impl ConfigReader {
    /// Creates a new `ConfigReader`.
    pub fn new() -> Self {
        Self {}
    }

    /// Reads and merges configuration from all sources, returning the resolved
    /// [`ForgeConfig`].
    ///
    /// Sources are applied in increasing priority order: embedded defaults,
    /// the optional file at `path` (skipped when `None`), then environment
    /// variables prefixed with `FORGE_`.
    pub async fn read(&self, path: Option<&Path>) -> crate::Result<ForgeConfig> {
        let defaults = include_str!("../.forge.toml");
        let mut builder = Config::builder();

        // Load default
        builder = builder.add_source(config::File::from_str(defaults, config::FileFormat::Toml));

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
                .try_parsing(true)
                .separator("_")
                .list_separator(","),
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
    use super::*;

    #[tokio::test]
    async fn test_read_parses_without_error() {
        let actual = ConfigReader::new().read(None).await;
        assert!(actual.is_ok(), "read() failed: {:?}", actual.err());
    }
}
