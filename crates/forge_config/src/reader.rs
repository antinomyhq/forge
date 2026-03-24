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
    /// # Panics
    ///
    /// Panics if the embedded default configuration cannot be parsed.
    pub async fn read(&self) -> ForgeConfig {
        todo!("implement multi-source config loading")
    }
}
