use std::collections::HashMap;

use derive_more::derive::From;
use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing as log;

use super::tool_call_parser::parse;
use crate::{Error, Result, ToolName};

/// Unique identifier for a using a tool
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ToolCallId(pub(crate) String);

impl ToolCallId {
    pub fn new(value: impl ToString) -> Self {
        ToolCallId(value.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Contains a part message for using a tool. This is received as a part of the
/// response from the model only when streaming is enabled.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Setters)]
#[setters(strip_option, into)]
pub struct ToolCallPart {
    /// Optional unique identifier that represents a single call to the tool
    /// use. NOTE: Not all models support a call ID for using a tool
    pub call_id: Option<ToolCallId>,
    pub name: Option<ToolName>,

    /// Arguments that need to be passed to the tool. NOTE: Not all tools
    /// require input
    pub arguments_part: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, From)]
pub enum ToolCall {
    Full(ToolCallFull),
    Part(ToolCallPart),
}

impl ToolCall {
    pub fn as_partial(&self) -> Option<&ToolCallPart> {
        match self {
            ToolCall::Full(_) => None,
            ToolCall::Part(part) => Some(part),
        }
    }

    pub fn as_full(&self) -> Option<&ToolCallFull> {
        match self {
            ToolCall::Full(full) => Some(full),
            ToolCall::Part(_) => None,
        }
    }
}

/// Contains the full information about using a tool. This is received as a part
/// of the response from the model when streaming is disabled.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Setters)]
#[setters(strip_option, into)]
#[serde(rename_all = "snake_case")]
pub struct ToolCallFull {
    pub name: ToolName,
    pub call_id: Option<ToolCallId>,
    pub arguments: Value,
}

impl ToolCallFull {
    pub fn new(tool_name: ToolName) -> Self {
        Self { name: tool_name, call_id: None, arguments: Value::default() }
    }

    pub fn try_from_parts(parts: &[ToolCallPart]) -> Result<Vec<Self>> {
        if parts.is_empty() {
            return Ok(vec![]);
        }

        // Use a more structured approach to track tool calls
        #[derive(Default)]
        struct ToolCallData {
            name: Option<ToolName>,
            call_id: Option<ToolCallId>,
            arguments_parts: Vec<String>,
        }

        // Group tool call parts by their call_id
        let mut calls_by_id: HashMap<String, ToolCallData> = HashMap::new();
        let mut current_id = "default_id".to_string();

        // First pass: Group parts by ID and collect names and argument parts
        for part in parts.iter() {
            // If we find a new call_id, update our tracking
            if let Some(id) = &part.call_id {
                current_id = id.as_str().to_string();
            }

            let entry = calls_by_id.entry(current_id.clone()).or_default();

            // Update name if present in this part
            if let Some(name) = &part.name {
                entry.name = Some(name.clone());
            }

            // Update call_id if present in this part
            if let Some(id) = &part.call_id {
                entry.call_id = Some(id.clone());
            }

            // Always add argument parts (even if they're empty)
            if !part.arguments_part.is_empty() {
                entry.arguments_parts.push(part.arguments_part.clone());
            }
        }

        // Second pass: Process each tool call with its collected data
        let mut tool_calls = Vec::new();

        for (_, data) in calls_by_id {
            if let Some(tool_name) = data.name {
                // Concatenate all argument fragments
                let args_json = data.arguments_parts.join("");

                // Only try to parse if we have some argument data
                if !args_json.is_empty() {
                    // Try to parse the arguments as JSON
                    let arguments = match serde_json::from_str(&args_json) {
                        Ok(args) => args,
                        Err(e) => {
                            // Log the error but include details about what we were trying to parse
                            log::debug!("Failed to parse tool call arguments: {}", e);
                            log::debug!("Arguments JSON (raw): {}", args_json);
                            return Err(Error::ToolCallFragmentParse {
                                error: e,
                                fragments: args_json,
                            });
                        }
                    };

                    tool_calls.push(ToolCallFull {
                        name: tool_name,
                        call_id: data.call_id,
                        arguments,
                    });
                } else {
                    // Handle the case where we have a name but no arguments
                    tool_calls.push(ToolCallFull {
                        name: tool_name,
                        call_id: data.call_id,
                        arguments: Value::default(),
                    });
                }
            }
        }

        if !tool_calls.is_empty() {
            Ok(tool_calls)
        } else {
            Err(Error::ToolCallMissingName)
        }
    }

    /// Parse multiple tool calls from XML format.
    pub fn try_from_xml(input: &str) -> std::result::Result<Vec<Self>, Error> {
        parse(input)
    }
}

#[cfg(test)]
mod tests {
    

    use super::*;

    #[test]
    fn test_multiple_calls() {
        let input = [
            ToolCallPart {
                call_id: Some(ToolCallId("call_1".to_string())),
                name: Some(ToolName::new("tool_forge_fs_read")),
                arguments_part: "{\"path\": \"crates/forge_app/src/fixtures/mascot.md\"}"
                    .to_string(),
            },
            ToolCallPart {
                call_id: Some(ToolCallId("call_2".to_string())),
                name: Some(ToolName::new("tool_forge_fs_read")),
                arguments_part: "{\"path\": \"docs/onboarding.md\"}".to_string(),
            },
            ToolCallPart {
                call_id: Some(ToolCallId("call_3".to_string())),
                name: Some(ToolName::new("tool_forge_fs_read")),
                arguments_part: "{\"path\": \"crates/forge_app/src/service/service.md\"}"
                    .to_string(),
            },
        ];

        let actual = ToolCallFull::try_from_parts(&input).unwrap();

        let exepected = vec![
            ToolCallFull {
                name: ToolName::new("tool_forge_fs_read"),
                call_id: Some(ToolCallId("call_1".to_string())),
                arguments: serde_json::json!({"path": "crates/forge_app/src/fixtures/mascot.md"}),
            },
            ToolCallFull {
                name: ToolName::new("tool_forge_fs_read"),
                call_id: Some(ToolCallId("call_2".to_string())),
                arguments: serde_json::json!({"path": "docs/onboarding.md"}),
            },
            ToolCallFull {
                name: ToolName::new("tool_forge_fs_read"),
                call_id: Some(ToolCallId("call_3".to_string())),
                arguments: serde_json::json!({"path": "crates/forge_app/src/service/service.md"}),
            },
        ];

        assert_eq!(actual, exepected);
    }

    #[test]
    fn test_single_tool_call() {
        let input = [ToolCallPart {
            call_id: Some(ToolCallId("call_1".to_string())),
            name: Some(ToolName::new("tool_forge_fs_read")),
            arguments_part: "{\"path\": \"docs/onboarding.md\"}".to_string(),
        }];

        let actual = ToolCallFull::try_from_parts(&input).unwrap();
        let expected = vec![ToolCallFull {
            call_id: Some(ToolCallId("call_1".to_string())),
            name: ToolName::new("tool_forge_fs_read"),
            arguments: serde_json::json!({"path": "docs/onboarding.md"}),
        }];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_empty_call_parts() {
        let actual = ToolCallFull::try_from_parts(&[]).unwrap();
        let expected = vec![];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fragmented_json_arguments() {
        // This test simulates the bug scenario where JSON is fragmented across multiple
        // parts
        let input = [
            ToolCallPart {
                call_id: Some(ToolCallId("call_1".to_string())),
                name: Some(ToolName::new("tool_forge_fs_create")),
                arguments_part: "{\"path\": \"".to_string(),
            },
            ToolCallPart {
                call_id: None,
                name: None,
                arguments_part: "/Users/test/".to_string(),
            },
            ToolCallPart {
                call_id: None,
                name: None,
                arguments_part: "crates/forge_ci/".to_string(),
            },
            ToolCallPart {
                call_id: None,
                name: None,
                arguments_part: "tests/ci.rs\",".to_string(),
            },
            ToolCallPart {
                call_id: None,
                name: None,
                arguments_part: "\"content\": \"test content\"}".to_string(),
            },
        ];

        let actual = ToolCallFull::try_from_parts(&input).unwrap();
        let expected = vec![ToolCallFull {
            call_id: Some(ToolCallId("call_1".to_string())),
            name: ToolName::new("tool_forge_fs_create"),
            arguments: serde_json::json!({
                "path": "/Users/test/crates/forge_ci/tests/ci.rs",
                "content": "test content"
            }),
        }];

        assert_eq!(actual, expected);
    }
}
