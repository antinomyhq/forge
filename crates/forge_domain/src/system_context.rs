use derive_setters::Setters;
use serde::{Deserialize, Serialize};

use crate::{Environment, File, Skill};

#[derive(Debug, Setters, Clone, Serialize, Deserialize)]
#[setters(strip_option)]
#[derive(Default)]
pub struct SystemContext {
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

    // List of files and directories that are relevant for the agent context
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<File>,

    #[serde(skip_serializing_if = "String::is_empty")]
    pub custom_rules: String,

    /// Indicates whether the agent supports parallel tool calls.
    #[serde(default)]
    pub supports_parallel_tool_calls: bool,

    /// List of available skills
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<Skill>,
}
