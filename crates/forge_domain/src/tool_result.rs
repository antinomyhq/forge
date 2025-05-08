use std::collections::HashMap;
use derive_setters::Setters;
use serde::{Deserialize, Serialize};

use crate::{ToolCallFull, ToolCallId, ToolName, tool_response::ToolResponseData};

#[derive(Clone, Debug, Deserialize, Serialize, Setters, PartialEq)]
#[setters(strip_option, into)]
pub struct ToolResult {
    pub name: ToolName,
    pub call_id: Option<ToolCallId>,
    #[setters(skip)]
    pub content: String,
    #[setters(skip)]
    pub is_error: bool,
    #[setters(skip)]
    #[serde(default)]
    pub data: ToolResponseData,
}

impl ToolResult {
    pub fn new(name: ToolName) -> ToolResult {
        Self {
            name,
            call_id: None,
            content: String::default(),
            is_error: false,
            data: ToolResponseData::default(),
        }
    }

    /// Sets the tool response data
    pub fn with_data(mut self, data: ToolResponseData) -> Self {
        self.data = data;
        self
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
}

impl From<ToolCallFull> for ToolResult {
    fn from(value: ToolCallFull) -> Self {
        Self {
            name: value.name,
            call_id: value.call_id,
            content: String::default(),
            is_error: false,
            data: ToolResponseData::default(),
        }
    }
}

impl std::fmt::Display for ToolResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {

        // Always add the tool_name and status to the metadata
        let data = match &self.data {
            ToolResponseData::FileRead { path, total_chars, char_range, is_binary } => {
                let mut map = HashMap::new();
                map.insert("type".to_string(), "file_read".into());
                map.insert("path".to_string(), path.clone().into());
                map.insert("tool_name".to_string(), serde_json::Value::String(self.name.as_str().to_string()));
                map.insert("status".to_string(), serde_json::Value::String(
                    if self.is_error { "Failure".to_string() } else { "Success".to_string() }
                ));

                if let Some(call_id) = &self.call_id {
                    map.insert("call_id".to_string(), serde_json::Value::String(call_id.as_str().to_string()));
                }

                if let Some(tc) = total_chars {
                    map.insert("total_chars".to_string(), (*tc).into());
                }

                if let Some((start, end)) = char_range {
                    map.insert("char_range".to_string(), format!("{}-{}", start, end).into());
                }

                if let Some(binary) = is_binary {
                    map.insert("is_binary".to_string(), (*binary).into());
                }

                ToolResponseData::Generic { metadata: map }
            },
            ToolResponseData::Shell { command, exit_code, total_chars, truncated } => {
                let mut map = HashMap::new();
                map.insert("type".to_string(), "shell".into());
                map.insert("command".to_string(), command.clone().into());
                map.insert("tool_name".to_string(), serde_json::Value::String(self.name.as_str().to_string()));
                map.insert("status".to_string(), serde_json::Value::String(
                    if self.is_error { "Failure".to_string() } else { "Success".to_string() }
                ));

                if let Some(call_id) = &self.call_id {
                    map.insert("call_id".to_string(), serde_json::Value::String(call_id.as_str().to_string()));
                }

                if let Some(code) = exit_code {
                    map.insert("exit_code".to_string(), (*code).into());
                }

                if let Some(tc) = total_chars {
                    map.insert("total_chars".to_string(), (*tc).into());
                }

                if let Some(t) = truncated {
                    map.insert("truncated".to_string(), (*t).into());
                }

                ToolResponseData::Generic { metadata: map }
            },
            ToolResponseData::FileWrite { path, bytes_written, created } => {
                let mut map = HashMap::new();
                map.insert("type".to_string(), "file_write".into());
                map.insert("path".to_string(), path.clone().into());
                map.insert("bytes_written".to_string(), (*bytes_written).into());
                map.insert("tool_name".to_string(), serde_json::Value::String(self.name.as_str().to_string()));
                map.insert("status".to_string(), serde_json::Value::String(
                    if self.is_error { "Failure".to_string() } else { "Success".to_string() }
                ));

                if let Some(call_id) = &self.call_id {
                    map.insert("call_id".to_string(), serde_json::Value::String(call_id.as_str().to_string()));
                }

                if let Some(c) = created {
                    map.insert("created".to_string(), (*c).into());
                }

                ToolResponseData::Generic { metadata: map }
            },
            ToolResponseData::WebSearch { query, result_count } => {
                let mut map = HashMap::new();
                map.insert("type".to_string(), "web_search".into());
                map.insert("query".to_string(), query.clone().into());
                map.insert("result_count".to_string(), (*result_count).into());
                map.insert("tool_name".to_string(), serde_json::Value::String(self.name.as_str().to_string()));
                map.insert("status".to_string(), serde_json::Value::String(
                    if self.is_error { "Failure".to_string() } else { "Success".to_string() }
                ));

                if let Some(call_id) = &self.call_id {
                    map.insert("call_id".to_string(), serde_json::Value::String(call_id.as_str().to_string()));
                }

                ToolResponseData::Generic { metadata: map }
            },
            ToolResponseData::WebFetch { url, status_code } => {
                let mut map = HashMap::new();
                map.insert("type".to_string(), "web_fetch".into());
                map.insert("url".to_string(), url.clone().into());
                map.insert("tool_name".to_string(), serde_json::Value::String(self.name.as_str().to_string()));
                map.insert("status".to_string(), serde_json::Value::String(
                    if self.is_error { "Failure".to_string() } else { "Success".to_string() }
                ));

                if let Some(call_id) = &self.call_id {
                    map.insert("call_id".to_string(), serde_json::Value::String(call_id.as_str().to_string()));
                }

                if let Some(code) = status_code {
                    map.insert("status_code".to_string(), (*code).into());
                }

                ToolResponseData::Generic { metadata: map }
            },
            ToolResponseData::CodebaseRetrieval { query, result_count } => {
                let mut map = HashMap::new();
                map.insert("type".to_string(), "codebase_retrieval".into());
                map.insert("query".to_string(), query.clone().into());
                map.insert("tool_name".to_string(), serde_json::Value::String(self.name.as_str().to_string()));
                map.insert("status".to_string(), serde_json::Value::String(
                    if self.is_error { "Failure".to_string() } else { "Success".to_string() }
                ));

                if let Some(call_id) = &self.call_id {
                    map.insert("call_id".to_string(), serde_json::Value::String(call_id.as_str().to_string()));
                }

                if let Some(count) = result_count {
                    map.insert("result_count".to_string(), (*count).into());
                }

                ToolResponseData::Generic { metadata: map }
            },
            ToolResponseData::Generic { metadata: existing } => {
                let mut map = existing.clone();
                map.insert("tool_name".to_string(), serde_json::Value::String(self.name.as_str().to_string()));
                map.insert("status".to_string(), serde_json::Value::String(
                    if self.is_error { "Failure".to_string() } else { "Success".to_string() }
                ));

                if let Some(call_id) = &self.call_id {
                    map.insert("call_id".to_string(), serde_json::Value::String(call_id.as_str().to_string()));
                }

                ToolResponseData::Generic { metadata: map }
            },
        };

        // Convert to front matter format
        let formatted = data.to_front_matter(&self.content);
        write!(f, "{}", formatted)
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
}
