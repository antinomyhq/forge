use std::collections::HashMap;

use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Environment;

#[derive(Debug, Setters, Clone, Serialize, Deserialize)]
#[setters(strip_option)]
#[derive(Default)]
pub struct SystemContext {
    // Current date and time at the time of context creation
    pub current_time: String,
    // Environment information to be included in the system context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<Environment>,

    // Information about available tools that can be used by the agent
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_information: Option<String>,

    /// Indicates whether the agent supports tools.
    /// This value is populated directly from the Agent configuration.
    #[serde(default)]
    pub tool_supported: bool,

    // List of file paths that are relevant for the agent context
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,

    #[serde(skip_serializing_if = "String::is_empty")]
    pub custom_rules: String,

    // Variables to pass to the system context
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub variables: HashMap<String, Value>,
}
