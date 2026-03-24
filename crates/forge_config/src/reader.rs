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
    /// `~/.forge/.forge.toml`, then environment variables prefixed with
    /// `FORGE_`.
    pub async fn read(&self) -> crate::Result<ForgeConfig> {
        // Embed the default config at compile time as the lowest-priority base.
        let defaults = include_str!("../.config.toml");
        let mut builder = Config::builder();

        // Load default
        builder = builder.add_source(config::File::from_str(defaults, config::FileFormat::Toml));

        // Load ~/.forge/.forge.toml
        if let Some(home_dir) = dirs::home_dir() {
            let home_config_path = home_dir.join(".forge").join(".forge.toml");
            if tokio::fs::try_exists(&home_config_path).await? {
                let contents = tokio::fs::read_to_string(&home_config_path).await?;
                builder =
                    builder.add_source(config::File::from_str(&contents, config::FileFormat::Toml));
            }
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
