use std::fs;

use anyhow::Result;
use forge_domain::ForgeConfig;

/// Load configuration from .forge file
pub fn load_forge_config() -> Result<ForgeConfig> {
    // Use absolute path to ensure consistency across components
    let current_dir = std::env::current_dir()?;
    let path = current_dir.join(".forge");

    if !path.exists() {
        // Return empty config if file doesn't exist
        return Ok(ForgeConfig::default());
    }

    let content = fs::read_to_string(&path)?;
    // Parse YAML content
    Ok(serde_yaml::from_str(&content)?)
}

/// Save configuration to .forge file
pub fn save_forge_config(config: &ForgeConfig) -> Result<()> {
    // Use absolute path to ensure consistency across components
    let current_dir = std::env::current_dir()?;
    let path = current_dir.join(".forge");

    // Convert to YAML
    let content = serde_yaml::to_string(config)?;

    // Write to file
    fs::write(&path, &content)?;

    // Verify file exists after write
    if path.exists() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "Failed to write .forge file to {}",
            path.display()
        ))
    }
}
