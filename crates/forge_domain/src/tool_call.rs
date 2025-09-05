use derive_more::derive::From;
use derive_setters::Setters;
use serde::{Deserialize, Serialize};

use crate::xml::extract_tag_content;
use crate::{Error, Result, ToolCallArguments, ToolName};

/// Unique identifier for a using a tool
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ToolCallId(pub(crate) String);

impl From<&str> for ToolCallId {
    fn from(value: &str) -> Self {
        ToolCallId(value.to_string())
    }
}

impl ToolCallId {
    pub fn new(value: impl ToString) -> Self {
        ToolCallId(value.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn generate() -> Self {
        let id = format!("forge_call_id_{}", uuid::Uuid::new_v4());
        ToolCallId(id)
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
    pub arguments: ToolCallArguments,
}

impl ToolCallFull {
    pub fn new(tool_name: impl Into<ToolName>) -> Self {
        Self {
            name: tool_name.into(),
            call_id: None,
            arguments: ToolCallArguments::default(),
        }
    }

    pub fn try_from_parts(parts: &[ToolCallPart]) -> Result<Vec<Self>> {
        if parts.is_empty() {
            return Ok(vec![]);
        }

        let mut tool_calls = Vec::new();
        let mut current_call_id: Option<ToolCallId> = None;
        let mut current_tool_name: Option<ToolName> = None;
        let mut current_arguments = String::new();

        for part in parts.iter() {
            // If we encounter a new call_id that's different from the current one,
            // finalize the previous tool call
            if let Some(new_call_id) = &part.call_id {
                if let Some(ref existing_call_id) = current_call_id
                    && existing_call_id.as_str() != new_call_id.as_str()
                {
                    // Finalize the previous tool call
                    if let Some(tool_name) = current_tool_name.take() {
                        let arguments = if current_arguments.is_empty() {
                            ToolCallArguments::default()
                        } else {
                            ToolCallArguments::from_json(current_arguments.as_str())
                        };

                        tool_calls.push(ToolCallFull {
                            name: tool_name,
                            call_id: Some(existing_call_id.clone()),
                            arguments,
                        });
                    }
                    current_arguments.clear();
                }
                current_call_id = Some(new_call_id.clone());
            }

            if let Some(name) = &part.name
                && !name.as_str().is_empty()
            {
                current_tool_name = Some(name.clone());
            }

            current_arguments.push_str(&part.arguments_part);
        }

        // Finalize the last tool call
        if let Some(tool_name) = current_tool_name {
            let arguments = if current_arguments.is_empty() {
                ToolCallArguments::default()
            } else {
                ToolCallArguments::from_json(current_arguments.as_str())
            };

            tool_calls.push(ToolCallFull { name: tool_name, call_id: current_call_id, arguments });
        }

        Ok(tool_calls)
    }

    /// Parse multiple tool calls from XML format.
    pub fn try_from_xml(input: &str) -> std::result::Result<Vec<ToolCallFull>, Error> {
        match extract_tag_content(input, "forge_tool_call") {
            None => Ok(Default::default()),
            Some(content) => {
                let mut tool_call: ToolCallFull =
                    json_repair_parse(content).map_err(|repair_error| Error::ToolCallArgument {
                        error: repair_error,
                        args: content.to_string(),
                    })?;

                // User might switch the model from a tool unsupported to tool supported model
                // leaving a lot of messages without tool calls

                tool_call.call_id = Some(ToolCallId::generate());
                Ok(vec![tool_call])
            }
        }
    }
}

fn json_repair_parse<T>(
    json_str: &str,
) -> std::result::Result<T, forge_json_repair::JsonRepairError>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_str(json_str)
        .map_err(forge_json_repair::JsonRepairError::JsonError)
        .or_else(|_| {
            let repaired = forge_json_repair::json_repair(json_str);
            if repaired.is_ok() {
                tracing::info!("Tool call was successfully repaired.");
            }
            repaired
        })
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_multiple_calls() {
        let input = [
            ToolCallPart {
                call_id: Some(ToolCallId("call_1".to_string())),
                name: Some(ToolName::new("read")),
                arguments_part: "{\"path\": \"crates/forge_services/src/fixtures/".to_string(),
            },
            ToolCallPart {
                call_id: None,
                name: None,
                arguments_part: "mascot.md\"}".to_string(),
            },
            ToolCallPart {
                call_id: Some(ToolCallId("call_2".to_string())),
                name: Some(ToolName::new("read")),
                arguments_part: "{\"path\": \"docs/".to_string(),
            },
            ToolCallPart {
                // NOTE: Call ID can be repeated with each message
                call_id: Some(ToolCallId("call_2".to_string())),
                name: None,
                arguments_part: "onboarding.md\"}".to_string(),
            },
            ToolCallPart {
                call_id: Some(ToolCallId("call_3".to_string())),
                name: Some(ToolName::new("read")),
                arguments_part: "{\"path\": \"crates/forge_services/src/service/".to_string(),
            },
            ToolCallPart {
                call_id: None,
                name: None,
                arguments_part: "service.md\"}".to_string(),
            },
        ];

        let actual = ToolCallFull::try_from_parts(&input).unwrap();

        let expected = vec![
            ToolCallFull {
                name: ToolName::new("read"),
                call_id: Some(ToolCallId("call_1".to_string())),
                arguments: ToolCallArguments::from_json(
                    r#"{"path": "crates/forge_services/src/fixtures/mascot.md"}"#,
                ),
            },
            ToolCallFull {
                name: ToolName::new("read"),
                call_id: Some(ToolCallId("call_2".to_string())),
                arguments: ToolCallArguments::from_json(r#"{"path": "docs/onboarding.md"}"#),
            },
            ToolCallFull {
                name: ToolName::new("read"),
                call_id: Some(ToolCallId("call_3".to_string())),
                arguments: ToolCallArguments::from_json(
                    r#"{"path": "crates/forge_services/src/service/service.md"}"#,
                ),
            },
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_single_tool_call() {
        let input = [ToolCallPart {
            call_id: Some(ToolCallId("call_1".to_string())),
            name: Some(ToolName::new("read")),
            arguments_part: "{\"path\": \"docs/onboarding.md\"}".to_string(),
        }];

        let actual = ToolCallFull::try_from_parts(&input).unwrap();
        let expected = vec![ToolCallFull {
            call_id: Some(ToolCallId("call_1".to_string())),
            name: ToolName::new("read"),
            arguments: ToolCallArguments::from_json(r#"{"path": "docs/onboarding.md"}"#),
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
    fn test_empty_arguments() {
        let input = [ToolCallPart {
            call_id: Some(ToolCallId("call_1".to_string())),
            name: Some(ToolName::new("screenshot")),
            arguments_part: "".to_string(),
        }];

        let actual = ToolCallFull::try_from_parts(&input).unwrap();
        let expected = vec![ToolCallFull {
            call_id: Some(ToolCallId("call_1".to_string())),
            name: ToolName::new("screenshot"),
            arguments: ToolCallArguments::default(),
        }];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_real_example() {
        let message = include_str!("./fixtures/tool_call_01.md");
        let tool_call = ToolCallFull::try_from_xml(message).unwrap();
        let actual = tool_call.first().unwrap().name.to_string();
        let expected = "attempt_completion";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_try_from_xml_call_id() {
        let message = include_str!("./fixtures/tool_call_01.md");
        let tool_call = ToolCallFull::try_from_xml(message).unwrap();
        let actual = tool_call.first().unwrap().call_id.as_ref().unwrap();
        assert!(actual.as_str().starts_with("forge_call_id_"));
    }
    #[test]
    fn test_try_from_parts_handles_empty_tool_names() {
        // Fixture: Tool call parts where empty names in subsequent parts should not
        // override valid names
        let input = [
            ToolCallPart {
                call_id: Some(ToolCallId("0".to_string())),
                name: Some(ToolName::new("read")),
                arguments_part: "".to_string(),
            },
            ToolCallPart {
                call_id: Some(ToolCallId("0".to_string())),
                name: Some(ToolName::new("")), // Empty name should not override valid name
                arguments_part: "{\"path\"".to_string(),
            },
            ToolCallPart {
                call_id: Some(ToolCallId("0".to_string())),
                name: Some(ToolName::new("")), // Empty name should not override valid name
                arguments_part: ": \"/test/file.md\"}".to_string(),
            },
        ];

        let actual = ToolCallFull::try_from_parts(&input).unwrap();

        let expected = vec![ToolCallFull {
            name: ToolName::new("read"),
            call_id: Some(ToolCallId("0".to_string())),
            arguments: ToolCallArguments::from_json(r#"{"path": "/test/file.md"}"#),
        }];

        assert_eq!(actual, expected);
    }
}
