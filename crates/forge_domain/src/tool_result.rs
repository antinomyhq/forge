use derive_setters::Setters;
use serde::{Deserialize, Serialize};

use crate::{ToolCallFull, ToolCallId, ToolName, ToolOutput};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Setters)]
#[setters(strip_option, into)]
pub struct ToolResult {
    pub name: ToolName,
    pub call_id: Option<ToolCallId>,
    pub output: ToolOutput,
}

impl ToolResult {
    pub fn new(name: ToolName) -> ToolResult {
        Self {
            name,
            call_id: Default::default(),
            output: Default::default(),
        }
    }

    pub fn success(mut self, content: impl Into<String>) -> Self {
        self.output = ToolOutput::text(content.into());

        self
    }

    pub fn failure(mut self, err: anyhow::Error) -> Self {
        let mut output = String::new();
        output.push_str("\nERROR:\n");

        for cause in err.chain() {
            output.push_str(&format!("Caused by: {cause}\n"));
        }

        self.output = ToolOutput::text(output).is_error(true);
        self
    }

    pub fn is_error(&self) -> bool {
        self.output.is_error
    }
}

impl From<ToolCallFull> for ToolResult {
    fn from(value: ToolCallFull) -> Self {
        Self {
            name: value.name,
            call_id: value.call_id,
            output: Default::default(),
        }
    }
}

impl std::fmt::Display for ToolResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<forge_tool_result>")?;
        write!(f, "<forge_tool_name>{}</forge_tool_name>", self.name)?;

        for content in self.output.items.iter().filter_map(|item| item.as_str()) {
            let content = format!("<![CDATA[{content}]]>");
            if self.is_error() {
                write!(f, "<error>{content}</error>")?;
            } else {
                write!(f, "<success>{content}</success>")?;
            }
        }
        write!(f, "</forge_tool_result>")
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub enum ToolOutputValue {
    Text(String),
    Base64URL(String),
    // FIXME: Drop this and use optional of ToolContentItem
    #[default]
    Empty,
}

impl ToolOutputValue {
    pub fn text(text: String) -> Self {
        ToolOutputValue::Text(text)
    }

    pub fn base64url(url: String) -> Self {
        ToolOutputValue::Base64URL(url)
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            ToolOutputValue::Text(text) => Some(text),
            ToolOutputValue::Base64URL(_) => None,
            ToolOutputValue::Empty => None,
        }
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
        assert!(!success.is_error());
        assert_eq!(format!("{:?}", success.output), "success message");

        let failure =
            ToolResult::new(ToolName::new("test_tool")).failure(anyhow::anyhow!("error message"));
        assert!(failure.is_error());
        assert_eq!(
            format!("{:?}", failure.output),
            "\nERROR:\nCaused by: error message\n"
        );
    }
}
