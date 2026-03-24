use std::path::Path;

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

    /// Serializes and writes the configuration to `path`, creating all parent
    /// directories recursively if they do not already exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration cannot be serialized or the file
    /// cannot be written.
    pub async fn write(&self, path: &Path) -> crate::Result<()> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let contents = toml_edit::ser::to_string_pretty(&self.config)?;

        tokio::fs::write(path, contents).await?;

        Ok(())
    }
}
