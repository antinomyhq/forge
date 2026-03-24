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
    /// the file at `path`, then environment variables prefixed with `FORGE_`.
    pub async fn read(&self, path: &Path) -> crate::Result<ForgeConfig> {
        // Embed the default config at compile time as the lowest-priority base.
        let defaults = include_str!("../.config.toml");
        let mut builder = Config::builder();

        // Load default
        builder = builder.add_source(config::File::from_str(defaults, config::FileFormat::Toml));

        // Load from path
        if tokio::fs::try_exists(path).await? {
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
}
