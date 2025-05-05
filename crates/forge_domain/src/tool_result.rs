use derive_setters::Setters;
use gray_matter::engine::YAML;
use gray_matter::{Matter, ParsedEntity};
use serde::{Deserialize, Serialize};

use crate::{ToolCallFull, ToolCallId, ToolName};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Setters)]
#[setters(strip_option, into)]
pub struct ToolResult {
    pub name: ToolName,
    pub call_id: Option<ToolCallId>,
    #[setters(skip)]
    pub content: String,
    #[setters(skip)]
    pub is_error: bool,
}

impl ToolResult {
    pub fn new(name: ToolName) -> ToolResult {
        Self {
            name,
            call_id: None,
            content: String::default(),
            is_error: false,
        }
    }

    pub fn success(mut self, content: impl Into<String>) -> Self {
        self.content = content.into();
        self.is_error = false;
        self
    }

    pub fn failure(mut self, err: anyhow::Error) -> Self {
        let mut output = String::new();
        output.push_str("\nERROR:\n");

        for cause in err.chain() {
            output.push_str(&format!("Caused by: {cause}\n"));
        }

        self.content = output;
        self.is_error = true;
        self
    }

    /// Serialize the tool result as a front matter document
    pub fn to_frontmatter(&self) -> String {
        let mut frontmatter = serde_json::Map::new();
        frontmatter.insert(
            "tool".to_string(),
            serde_json::Value::String(self.name.as_str().to_string()),
        );

        if let Some(call_id) = &self.call_id {
            frontmatter.insert(
                "call_id".to_string(),
                serde_json::Value::String(call_id.as_str().to_string()),
            );
        }

        frontmatter.insert(
            "is_error".to_string(),
            serde_json::Value::Bool(self.is_error),
        );

        let frontmatter_value = serde_json::Value::Object(frontmatter);
        let frontmatter_yaml = serde_yml::to_string(&frontmatter_value).unwrap_or_default();

        format!("---\n{}---\n{}", frontmatter_yaml, self.content)
    }

    /// Parse a front matter document back into a ToolResult
    pub fn from_frontmatter(content: &str) -> anyhow::Result<Self> {
        let matter = Matter::<YAML>::new();
        let parsed: ParsedEntity = matter.parse(content);

        let data = parsed
            .data
            .ok_or_else(|| anyhow::anyhow!("No front matter found"))?;

        let name = data["tool"]
            .as_string()
            .map_err(|_| anyhow::anyhow!("Invalid tool name"))?;
        let name = ToolName::new(name);

        let call_id = match data["call_id"].as_string() {
            Ok(id) => Some(ToolCallId::new(id)),
            Err(_) => None,
        };

        let is_error = data["is_error"].as_bool().unwrap_or(false);

        Ok(Self { name, call_id, content: parsed.content, is_error })
    }
}

impl From<ToolCallFull> for ToolResult {
    fn from(value: ToolCallFull) -> Self {
        Self {
            name: value.name,
            call_id: value.call_id,
            content: String::default(),
            is_error: false,
        }
    }
}

impl std::fmt::Display for ToolResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_frontmatter())
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_snapshot_minimal() {
        let result = ToolResult::new(ToolName::new("test_tool"));
        assert_snapshot!(result);
    }

    #[test]
    fn test_snapshot_full() {
        let result = ToolResult::new(ToolName::new("complex_tool"))
            .call_id(ToolCallId::new("123"))
            .failure(anyhow::anyhow!(
                json!({"key": "value", "number": 42}).to_string()
            ));
        assert_snapshot!(result);
    }

    #[test]
    fn test_snapshot_with_special_chars() {
        let result = ToolResult::new(ToolName::new("xml_tool")).success(
            json!({
                "text": "Special chars: < > & ' \"",
                "nested": {
                    "html": "<div>Test</div>"
                }
            })
            .to_string(),
        );
        assert_snapshot!(result);
    }

    #[test]
    fn test_display_minimal() {
        let result = ToolResult::new(ToolName::new("test_tool"));
        assert_snapshot!(result.to_string());
    }

    #[test]
    fn test_display_full() {
        let result = ToolResult::new(ToolName::new("complex_tool"))
            .call_id(ToolCallId::new("123"))
            .success(
                json!({
                    "user": "John Doe",
                    "age": 42,
                    "address": [{"city": "New York"}, {"city": "Los Angeles"}]
                })
                .to_string(),
            );
        assert_snapshot!(result.to_string());
    }

    #[test]
    fn test_display_special_chars() {
        let result = ToolResult::new(ToolName::new("xml_tool")).success(
            json!({
                "text": "Special chars: < > & ' \"",
                "nested": {
                    "html": "<div>Test</div>"
                }
            })
            .to_string(),
        );
        assert_snapshot!(result.to_string());
    }

    #[test]
    fn test_success_and_failure_content() {
        let success = ToolResult::new(ToolName::new("test_tool")).success("success message");
        assert!(!success.is_error);
        assert_eq!(success.content, "success message");

        let failure =
            ToolResult::new(ToolName::new("test_tool")).failure(anyhow::anyhow!("error message"));
        assert!(failure.is_error);
        assert_eq!(failure.content, "\nERROR:\nCaused by: error message\n");
    }

    #[test]
    fn test_frontmatter_roundtrip() {
        let original = ToolResult::new(ToolName::new("test_tool"))
            .call_id(ToolCallId::new("abc123"))
            .success("This is the content\nWith multiple lines");

        let frontmatter = original.to_frontmatter();
        let parsed = ToolResult::from_frontmatter(&frontmatter).unwrap();

        assert_eq!(parsed.name, original.name);
        assert_eq!(parsed.call_id, original.call_id);
        assert_eq!(parsed.content, original.content);
        assert_eq!(parsed.is_error, original.is_error);
    }
}
