use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Represents the configuration stored in a `.forge` file
#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq)]
pub struct ForgeConfig {
    /// Model mappings for different roles
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<HashMap<String, String>>,

    /// Provider configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<ProviderConfig>,

    /// Tool support configuration for each model type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_support: Option<HashMap<String, bool>>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct ProviderConfig {
    pub provider_type: String,
    // We don't store the API key in the config file for security reasons
    // It will be stored in .env instead
}
