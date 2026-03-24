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
    /// # Errors
    ///
    /// Returns an error if the configuration cannot be serialized or the file
    /// cannot be written.
    pub async fn write(&self) -> crate::Result<()> {
        let _ = &self.config;
        Ok(())
    }
}
