use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, derive_more::Display)]
#[serde(transparent)]
pub struct ToolName(String);

impl ToolName {
    pub fn new(value: impl ToString) -> Self {
        ToolName(value.to_string())
    }
    pub fn prefixed(prefix: impl ToString, tool_name: impl ToString) -> Self {
        let prefix = prefix
            .to_string()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect::<String>();
        let prefix = if prefix.len() > 10 {
            prefix[prefix.len() - 10..].to_string()
        } else {
            prefix
        };

        let input = format!("{}-forgestrip-{}", prefix, tool_name.to_string());

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

    pub fn strip_prefix(&self) -> String {
        let split = self.0.split("-forgestrip-").collect::<Vec<&str>>();
        split.get(1).unwrap_or(&self.0.as_str()).to_string()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub trait NamedTool {
    fn tool_name() -> ToolName;
}
