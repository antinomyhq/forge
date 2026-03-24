use crate::ForgeConfig;

/// Writes a [`ForgeConfig`] to the user configuration file on disk.
pub struct ConfigWriter {
    config: ForgeConfig,
}

impl ConfigWriter {
    /// Creates a new `ConfigWriter` for the given configuration.
    pub fn new(config: ForgeConfig) -> Self {
        Self { config }
    }

    /// Serializes and writes the configuration to the user config file.
    ///
    /// Writes to `~/.forge/.forge.toml`, creating the parent directory if it
    /// does not already exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration cannot be serialized or the file
    /// cannot be written.
    pub async fn write(&self) -> crate::Result<()> {
        let home_dir = dirs::home_dir().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "home directory not found")
        })?;
        let config_dir = home_dir.join(".forge");
        let config_path = config_dir.join(".forge.toml");

        tokio::fs::create_dir_all(&config_dir).await?;

        let contents = toml_edit::ser::to_string_pretty(&self.config)?;

        tokio::fs::write(&config_path, contents).await?;

        Ok(())
    }
}
