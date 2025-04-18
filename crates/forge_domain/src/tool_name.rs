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

#[cfg(test)]
mod tess {
    use crate::ToolName;

    #[test]
    fn test_prefixed_basic() {
        let name = ToolName::prefixed("my_prefix", "tool");
        assert!(name.as_str().starts_with("my_prefix-forgestrip-tool"));
        assert_eq!(name.strip_prefix(), "tool");
    }

    #[test]
    fn test_prefixed_filters_invalid_chars() {
        let name = ToolName::prefixed("!@#bad$$prefix", "some*tool");
        assert!(name.as_str().contains("badprefix-forgestrip-sometool"));
    }

    #[test]
    fn test_prefixed_truncates_long_name() {
        let long_prefix = "verylongprefixnameexceedingtencharacters";
        let name = ToolName::prefixed(long_prefix, "supertool");
        assert!(name.as_str().len() <= 64);
        assert!(name.as_str().contains("supertool"));
    }

    #[test]
    fn test_strip_prefix_exists() {
        let name = ToolName::new("abc-forgestrip-mytool");
        assert_eq!(name.strip_prefix(), "mytool");
    }

    #[test]
    fn test_strip_prefix_absent() {
        let name = ToolName::new("mytool");
        assert_eq!(name.strip_prefix(), "mytool");
    }

    #[test]
    fn test_into_string() {
        let name = ToolName::new("converted_tool");
        let string = name.clone().into_string();
        assert_eq!(string, "converted_tool");
    }

    #[test]
    fn test_serialization() {
        let tool = ToolName::new("serialize_tool");
        let json = serde_json::to_string(&tool).unwrap();
        assert_eq!(json, "\"serialize_tool\"");
        let deserialized: ToolName = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, tool);
    }
}
