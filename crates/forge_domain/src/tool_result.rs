use derive_setters::Setters;
use serde::{Deserialize, Serialize};

use crate::{ToolCallFull, ToolCallId, ToolName};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolResponseData {
    FileRead {
        path: String,
        content: String,
    },
    FileWrite {
        path: String,
        success: bool,
        bytes_written: usize,
    },
    Shell {
        command: String,
        exit_code: i32,
        stdout: String,
        stderr: String,
    },
    Generic {
        content: String,
    },
}

impl ToolResponseData {
    pub fn to_front_matter(&self) -> String {
        let (body, meta) = match self {
            ToolResponseData::Generic { content } => (content.clone(), self),
            ToolResponseData::FileRead { content, .. } => (content.clone(), self),
            ToolResponseData::FileWrite { .. } => {
                ("File write operation completed.".to_string(), self)
            }
            ToolResponseData::Shell { stdout, stderr, .. } => {
                (format!("STDOUT:\n{stdout}\nSTDERR:\n{stderr}"), self)
            }
        };

        let yaml =
            serde_yaml::to_string(meta).expect("ToolResponseData must be serializable to YAML");
        format!("---\n{yaml}---\n{body}")
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Setters)]
#[setters(strip_option, into)]
pub struct ToolResult {
    pub name: ToolName,
    pub call_id: Option<ToolCallId>,
    #[setters(skip)]
    pub data: ToolResponseData,
    #[setters(skip)]
    pub is_error: bool,
}

impl ToolResult {
    pub fn new(name: ToolName) -> ToolResult {
        Self {
            name,
            call_id: None,
            data: ToolResponseData::Generic { content: String::new() },
            is_error: false,
        }
    }

    pub fn success(mut self, data: ToolResponseData) -> Self {
        self.data = data;
        self.is_error = false;
        self
    }

    pub fn failure(mut self, err: anyhow::Error) -> Self {
        let mut output = String::new();
        output.push_str("\nERROR:\n");

        for cause in err.chain() {
            output.push_str(&format!("Caused by: {cause}\n"));
        }
        self.data = ToolResponseData::Generic { content: output };
        self.is_error = true;
        self
    }

    pub fn content(&self) -> String {
        match &self.data {
            ToolResponseData::Generic { content } => content.clone(),
            ToolResponseData::FileRead { content, .. } => content.clone(),
            ToolResponseData::Shell { stdout, stderr, .. } => {
                format!("STDOUT:\n{stdout}\nSTDERR:\n{stderr}")
            }
            ToolResponseData::FileWrite { .. } => "File write operation completed.".to_string(),
        }
    }
}

impl From<ToolCallFull> for ToolResult {
    fn from(value: ToolCallFull) -> Self {
        Self {
            name: value.name,
            call_id: value.call_id,
            data: ToolResponseData::Generic { content: String::new() },
            is_error: false,
        }
    }
}

impl std::fmt::Display for ToolResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<forge_tool_result>")?;
        write!(
            f,
            "<forge_tool_name>{}</forge_tool_name>",
            self.name.as_str()
        )?;
        let cdata = format!("<![CDATA[{}]]>", self.data.to_front_matter());
        if self.is_error {
            write!(f, "<error>{cdata}</error>")?;
        } else {
            write!(f, "<success>{cdata}</success>")?;
        }
        write!(f, "</forge_tool_result>")
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
        let result =
            ToolResult::new(ToolName::new("xml_tool")).success(ToolResponseData::Generic {
                content: json!({
                    "text": "Special chars: < > & ' \"",
                    "nested": {
                        "html": "<div>Test</div>"
                    }
                })
                .to_string(),
            });
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
            .success(ToolResponseData::Generic {
                content: json!({
                    "user": "John Doe",
                    "age": 42,
                    "address": [{"city": "New York"}, {"city": "Los Angeles"}]
                })
                .to_string(),
            });
        assert_snapshot!(result.to_string());
    }

    #[test]
    fn test_display_special_chars() {
        let result =
            ToolResult::new(ToolName::new("xml_tool")).success(ToolResponseData::Generic {
                content: json!({
                    "text": "Special chars: < > & ' \"",
                    "nested": {
                        "html": "<div>Test</div>"
                    }
                })
                .to_string(),
            });
        assert_snapshot!(result.to_string());
    }

    #[test]
    fn test_success_and_failure_content() {
        let success = ToolResult::new(ToolName::new("test_tool"))
            .success(ToolResponseData::Generic { content: "success message".to_string() });
        assert!(!success.is_error);
        assert_eq!(success.content(), "success message");

        let failure =
            ToolResult::new(ToolName::new("test_tool")).failure(anyhow::anyhow!("error message"));
        assert!(failure.is_error);
        assert_eq!(failure.content(), "\nERROR:\nCaused by: error message\n");
    }
}
