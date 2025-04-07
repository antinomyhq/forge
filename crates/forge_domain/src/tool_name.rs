use derive_more::Display;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
#[serde(transparent)]
pub struct ToolName(String);

impl ToolName {
    pub fn new(value: impl ToString) -> Self {
        ToolName(value.to_string())
    }
    pub fn prefixed(prefix: impl ToString, tool_name: impl ToString) -> Self {
        let input = format!("{}-{}", prefix.to_string(), tool_name.to_string());

        if input.is_empty() {
            panic!("Input string cannot be null or empty");
        }

        // Keep only alphanumeric characters, underscores, or hyphens
        let formatted: String = input
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect();

        // Truncate to the last 64 characters if longer
        if formatted.len() > 64 {
            ToolName(formatted[formatted.len() - 64..].to_string())
        } else {
            ToolName(formatted)
        }
    }
}

impl ToolName {
    pub fn into_string(self) -> String {
        self.0
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub trait NamedTool {
    fn tool_name() -> ToolName;
}
