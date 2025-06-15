use derive_more::derive::{Display, From};
use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::{ToolCallFull, ToolResult};
use crate::temperature::Temperature;
use crate::top_k::TopK;
use crate::top_p::TopP;
use crate::{ConversationId, Image, ModelId, ToolChoice, ToolDefinition, ToolValue};

/// Represents a message being sent to the LLM provider
/// NOTE: ToolResults message are part of the larger Request object and not part
/// of the message.
#[derive(Clone, Debug, Deserialize, From, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ContextMessage {
    Text(TextMessage),
    Tool(ToolResult),
    Image(Image),
}

impl ContextMessage {
    /// Estimates the number of tokens in a message using character-based
    /// approximation.
    /// ref: https://github.com/openai/codex/blob/main/codex-cli/src/utils/approximate-tokens-used.ts
    pub fn count_tokens(&self) -> u64 {
        let char_count = match self {
            ContextMessage::Text(text_message)
                if matches!(text_message.role, Role::User | Role::Assistant) =>
            {
                text_message.content.chars().count()
                    + text_message
                        .tool_calls
                        .as_ref()
                        .map(|tool_calls| {
                            tool_calls
                                .iter()
                                .map(|tc| {
                                    tc.arguments.to_string().chars().count()
                                        + tc.name.as_str().chars().count()
                                })
                                .sum()
                        })
                        .unwrap_or(0)
            }
            ContextMessage::Tool(tool_result) => tool_result
                .output
                .values
                .iter()
                .map(|result| match result {
                    ToolValue::Text(text) => text.chars().count(),
                    _ => 0,
                })
                .sum(),
            _ => 0,
        };
        (char_count as f64 / 4.0).ceil() as u64
    }

    pub fn to_text(&self) -> String {
        let mut lines = String::new();
        match self {
            ContextMessage::Text(message) => {
                lines.push_str(&format!("<message role=\"{}\">", message.role));
                lines.push_str(&format!("<content>{}</content>", message.content));
                if let Some(tool_calls) = &message.tool_calls {
                    for call in tool_calls {
                        lines.push_str(&format!(
                            "<forge_tool_call name=\"{}\"><![CDATA[{}]]></forge_tool_call>",
                            call.name,
                            serde_json::to_string(&call.arguments).unwrap()
                        ));
                    }
                }

                lines.push_str("</message>");
            }
            ContextMessage::Tool(result) => {
                lines.push_str("<message role=\"tool\">");

                lines.push_str(&format!(
                    "<forge_tool_result name=\"{}\"><![CDATA[{}]]></forge_tool_result>",
                    result.name,
                    serde_json::to_string(&result.output).unwrap()
                ));
                lines.push_str("</message>");
            }
            ContextMessage::Image(_) => {
                lines.push_str("<image path=\"[base64 URL]\">".to_string().as_str());
            }
        }
        lines
    }

    pub fn user(content: impl ToString, model: Option<ModelId>) -> Self {
        TextMessage {
            role: Role::User,
            content: content.to_string(),
            tool_calls: None,
            model,
        }
        .into()
    }

    pub fn system(content: impl ToString) -> Self {
        TextMessage {
            role: Role::System,
            content: content.to_string(),
            tool_calls: None,
            model: None,
        }
        .into()
    }

    pub fn assistant(content: impl ToString, tool_calls: Option<Vec<ToolCallFull>>) -> Self {
        let tool_calls =
            tool_calls.and_then(|calls| if calls.is_empty() { None } else { Some(calls) });
        TextMessage {
            role: Role::Assistant,
            content: content.to_string(),
            tool_calls,
            model: None,
        }
        .into()
    }

    pub fn tool_result(result: ToolResult) -> Self {
        Self::Tool(result)
    }

    pub fn has_role(&self, role: Role) -> bool {
        match self {
            ContextMessage::Text(message) => message.role == role,
            ContextMessage::Tool(_) => false,
            ContextMessage::Image(_) => Role::User == role,
        }
    }

    pub fn has_tool_call(&self) -> bool {
        match self {
            ContextMessage::Text(message) => message.tool_calls.is_some(),
            ContextMessage::Tool(_) => false,
            ContextMessage::Image(_) => false,
        }
    }
}

//TODO: Rename to TextMessage
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Setters)]
#[setters(strip_option, into)]
#[serde(rename_all = "snake_case")]
pub struct TextMessage {
    pub role: Role,
    pub content: String,
    pub tool_calls: Option<Vec<ToolCallFull>>,
    // note: this used to track model used for this message.
    pub model: Option<ModelId>,
}

impl TextMessage {
    pub fn assistant(content: impl ToString, model: Option<ModelId>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.to_string(),
            tool_calls: None,
            model,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Display)]
pub enum Role {
    System,
    User,
    Assistant,
}

/// Represents a request being made to the LLM provider. By default the request
/// is created with assuming the model supports use of external tools.
#[derive(Clone, Debug, Deserialize, Serialize, Setters, Default, PartialEq)]
#[setters(into, strip_option)]
pub struct Context {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<ConversationId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<ContextMessage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<Temperature>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<TopP>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<TopK>,
}

impl Context {
    pub fn message_groups(&self, skip: Option<usize>) -> Vec<Vec<&ContextMessage>> {
        let mut groups = Vec::new();
        let mut current_group: Option<Vec<&ContextMessage>> = None;
        let skip = skip.unwrap_or(0);

        let mut iter = self.messages.iter().skip(skip).peekable();

        while let Some(message) = iter.next() {
            match message {
                // Assistant with tool calls starts a new group
                ContextMessage::Text(text) if Self::is_assistant_with_tools(text) => {
                    // Finish any existing group first
                    if let Some(group) = current_group.take() {
                        groups.push(group);
                    }
                    // Start new group with this message
                    current_group = Some(vec![message]);
                }
                // Tool results are added to current group if one exists, else ignored as tool
                // result will always comes with it's tool call.
                ContextMessage::Tool(_) => {
                    if let Some(ref mut group) = current_group {
                        group.push(message);
                    }

                    // If it's the last message, finish any existing group
                    if iter.peek().is_none() {
                        if let Some(group) = current_group.take() {
                            groups.push(group);
                        }
                    }
                }
                // Any other message: finish current group and add this as single message
                _ => {
                    // Finish any existing group first
                    if let Some(group) = current_group.take() {
                        groups.push(group);
                    }
                    // Add this message as a single-message group
                    groups.push(vec![message]);
                }
            }
        }

        groups
    }

    /// Checks if a text message is an assistant message with tool calls
    fn is_assistant_with_tools(text_message: &TextMessage) -> bool {
        text_message.role == Role::Assistant
            && text_message
                .tool_calls
                .as_ref()
                .is_some_and(|calls| !calls.is_empty())
    }

    pub fn add_base64_url(mut self, image: Image) -> Self {
        self.messages.push(ContextMessage::Image(image));
        self
    }

    pub fn add_tool(mut self, tool: impl Into<ToolDefinition>) -> Self {
        let tool: ToolDefinition = tool.into();
        self.tools.push(tool);
        self
    }

    pub fn add_message(mut self, content: impl Into<ContextMessage>) -> Self {
        let content = content.into();
        debug!(content = ?content, "Adding message to context");
        self.messages.push(content);

        self
    }

    pub fn add_tool_results(mut self, results: Vec<ToolResult>) -> Self {
        if !results.is_empty() {
            debug!(results = ?results, "Adding tool results to context");
            self.messages
                .extend(results.into_iter().map(ContextMessage::tool_result));
        }

        self
    }

    /// Updates the set system message
    pub fn set_first_system_message(mut self, content: impl Into<String>) -> Self {
        if self.messages.is_empty() {
            self.add_message(ContextMessage::system(content.into()))
        } else {
            if let Some(ContextMessage::Text(content_message)) = self.messages.get_mut(0) {
                if content_message.role == Role::System {
                    content_message.content = content.into();
                } else {
                    self.messages
                        .insert(0, ContextMessage::system(content.into()));
                }
            }

            self
        }
    }

    /// Converts the context to textual format
    pub fn to_text(&self) -> String {
        let mut lines = String::new();

        for message in self.messages.iter() {
            lines.push_str(&message.to_text());
        }

        format!("<chat_history>{lines}</chat_history>")
    }

    /// Will append a message to the context. This method always assumes tools
    /// are supported and uses the appropriate format. For models that don't
    /// support tools, use the TransformToolCalls transformer to convert the
    /// context afterward.
    pub fn append_message(
        self,
        content: impl ToString,
        tool_records: Vec<(ToolCallFull, ToolResult)>,
    ) -> Self {
        // Adding tool calls
        self.add_message(ContextMessage::assistant(
            content,
            Some(
                tool_records
                    .iter()
                    .map(|record| record.0.clone())
                    .collect::<Vec<_>>(),
            ),
        ))
        // Adding tool results
        .add_tool_results(
            tool_records
                .iter()
                .map(|record| record.1.clone())
                .collect::<Vec<_>>(),
        )
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_yaml_snapshot;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::estimate_token_count;
    use crate::transformer::Transformer;

    #[test]
    fn test_override_system_message() {
        let request = Context::default()
            .add_message(ContextMessage::system("Initial system message"))
            .set_first_system_message("Updated system message");

        assert_eq!(
            request.messages[0],
            ContextMessage::system("Updated system message"),
        );
    }

    #[test]
    fn test_set_system_message() {
        let request = Context::default().set_first_system_message("A system message");

        assert_eq!(
            request.messages[0],
            ContextMessage::system("A system message"),
        );
    }

    #[test]
    fn test_insert_system_message() {
        let model = ModelId::new("test-model");
        let request = Context::default()
            .add_message(ContextMessage::user("Do something", Some(model)))
            .set_first_system_message("A system message");

        assert_eq!(
            request.messages[0],
            ContextMessage::system("A system message"),
        );
    }

    #[test]
    fn test_estimate_token_count() {
        // Create a context with some messages
        let model = ModelId::new("test-model");
        let context = Context::default()
            .add_message(ContextMessage::system("System message"))
            .add_message(ContextMessage::user("User message", model.into()))
            .add_message(ContextMessage::assistant("Assistant message", None));

        // Get the token count
        let token_count = estimate_token_count(context.to_text().len());

        // Validate the token count is reasonable
        // The exact value will depend on the implementation of estimate_token_count
        assert!(token_count > 0, "Token count should be greater than 0");
    }

    #[test]
    fn test_update_image_tool_calls_empty_context() {
        let fixture = Context::default();
        let mut transformer = crate::transformer::ImageHandling::new();
        let actual = transformer.transform(fixture);

        assert_yaml_snapshot!(actual);
    }

    #[test]
    fn test_update_image_tool_calls_no_tool_results() {
        let fixture = Context::default()
            .add_message(ContextMessage::system("System message"))
            .add_message(ContextMessage::user("User message", None))
            .add_message(ContextMessage::assistant("Assistant message", None));
        let mut transformer = crate::transformer::ImageHandling::new();
        let actual = transformer.transform(fixture);

        assert_yaml_snapshot!(actual);
    }

    #[test]
    fn test_update_image_tool_calls_tool_results_no_images() {
        let fixture = Context::default()
            .add_message(ContextMessage::system("System message"))
            .add_tool_results(vec![
                ToolResult {
                    name: crate::ToolName::new("text_tool"),
                    call_id: Some(crate::ToolCallId::new("call1")),
                    output: crate::ToolOutput::text("Text output".to_string()),
                },
                ToolResult {
                    name: crate::ToolName::new("empty_tool"),
                    call_id: Some(crate::ToolCallId::new("call2")),
                    output: crate::ToolOutput {
                        values: vec![crate::ToolValue::Empty],
                        is_error: false,
                    },
                },
            ]);

        let mut transformer = crate::transformer::ImageHandling::new();
        let actual = transformer.transform(fixture);

        assert_yaml_snapshot!(actual);
    }

    #[test]
    fn test_update_image_tool_calls_single_image() {
        let image = Image::new_base64("test123".to_string(), "image/png");
        let fixture = Context::default()
            .add_message(ContextMessage::system("System message"))
            .add_tool_results(vec![ToolResult {
                name: crate::ToolName::new("image_tool"),
                call_id: Some(crate::ToolCallId::new("call1")),
                output: crate::ToolOutput::image(image),
            }]);

        let mut transformer = crate::transformer::ImageHandling::new();
        let actual = transformer.transform(fixture);

        assert_yaml_snapshot!(actual);
    }

    #[test]
    fn test_update_image_tool_calls_multiple_images_single_tool_result() {
        let image1 = Image::new_base64("test123".to_string(), "image/png");
        let image2 = Image::new_base64("test456".to_string(), "image/jpeg");
        let fixture = Context::default().add_tool_results(vec![ToolResult {
            name: crate::ToolName::new("multi_image_tool"),
            call_id: Some(crate::ToolCallId::new("call1")),
            output: crate::ToolOutput {
                values: vec![
                    crate::ToolValue::Text("First text".to_string()),
                    crate::ToolValue::Image(image1),
                    crate::ToolValue::Text("Second text".to_string()),
                    crate::ToolValue::Image(image2),
                ],
                is_error: false,
            },
        }]);

        let mut transformer = crate::transformer::ImageHandling::new();
        let actual = transformer.transform(fixture);

        assert_yaml_snapshot!(actual);
    }

    #[test]
    fn test_update_image_tool_calls_multiple_tool_results_with_images() {
        let image1 = Image::new_base64("test123".to_string(), "image/png");
        let image2 = Image::new_base64("test456".to_string(), "image/jpeg");
        let fixture = Context::default()
            .add_message(ContextMessage::system("System message"))
            .add_tool_results(vec![
                ToolResult {
                    name: crate::ToolName::new("text_tool"),
                    call_id: Some(crate::ToolCallId::new("call1")),
                    output: crate::ToolOutput::text("Text output".to_string()),
                },
                ToolResult {
                    name: crate::ToolName::new("image_tool1"),
                    call_id: Some(crate::ToolCallId::new("call2")),
                    output: crate::ToolOutput::image(image1),
                },
                ToolResult {
                    name: crate::ToolName::new("image_tool2"),
                    call_id: Some(crate::ToolCallId::new("call3")),
                    output: crate::ToolOutput::image(image2),
                },
            ]);

        let mut transformer = crate::transformer::ImageHandling::new();
        let actual = transformer.transform(fixture);

        assert_yaml_snapshot!(actual);
    }

    #[test]
    fn test_update_image_tool_calls_mixed_content_with_images() {
        let image = Image::new_base64("test123".to_string(), "image/png");
        let fixture = Context::default()
            .add_message(ContextMessage::system("System message"))
            .add_message(ContextMessage::user("User question", None))
            .add_message(ContextMessage::assistant("Assistant response", None))
            .add_tool_results(vec![ToolResult {
                name: crate::ToolName::new("mixed_tool"),
                call_id: Some(crate::ToolCallId::new("call1")),
                output: crate::ToolOutput {
                    values: vec![
                        crate::ToolValue::Text("Before image".to_string()),
                        crate::ToolValue::Image(image),
                        crate::ToolValue::Text("After image".to_string()),
                        crate::ToolValue::Empty,
                    ],
                    is_error: false,
                },
            }]);

        let mut transformer = crate::transformer::ImageHandling::new();
        let actual = transformer.transform(fixture);

        assert_yaml_snapshot!(actual);
    }

    #[test]
    fn test_update_image_tool_calls_preserves_error_flag() {
        let image = Image::new_base64("test123".to_string(), "image/png");
        let fixture = Context::default().add_tool_results(vec![ToolResult {
            name: crate::ToolName::new("error_tool"),
            call_id: Some(crate::ToolCallId::new("call1")),
            output: crate::ToolOutput {
                values: vec![crate::ToolValue::Image(image)],
                is_error: true,
            },
        }]);

        let mut transformer = crate::transformer::ImageHandling::new();
        let actual = transformer.transform(fixture);

        assert_yaml_snapshot!(actual);
    }
    #[test]
    fn test_message_iter_groups_tool_calls_and_results() {
        use crate::{ToolCallFull, ToolCallId, ToolName, ToolOutput, ToolResult};

        let fixture = Context::default()
            .add_message(ContextMessage::system("System message"))
            .add_message(ContextMessage::user("User message", None))
            .add_message(ContextMessage::assistant(
                "I'll use a tool",
                Some(vec![ToolCallFull {
                    call_id: Some(ToolCallId::new("call1")),
                    name: ToolName::new("test_tool"),
                    arguments: serde_json::json!({"param": "value"}),
                }]),
            ))
            .add_tool_results(vec![ToolResult {
                name: ToolName::new("test_tool"),
                call_id: Some(ToolCallId::new("call1")),
                output: ToolOutput::text("Tool result".to_string()),
            }])
            .add_message(ContextMessage::assistant("Final response", None));

        let actual = fixture.message_groups(None);

        // Should have 4 groups:
        // 1. System message
        // 2. User message
        // 3. Assistant with tool call + tool result
        // 4. Final assistant message
        assert_eq!(actual.len(), 4);
        assert_eq!(actual[0].len(), 1); // System message alone
        assert_eq!(actual[1].len(), 1); // User message alone
        assert_eq!(actual[2].len(), 2); // Assistant + tool result together
        assert_eq!(actual[3].len(), 1); // Final assistant message alone
    }

    #[test]
    fn test_message_iter_single_messages() {
        let fixture = Context::default()
            .add_message(ContextMessage::system("System"))
            .add_message(ContextMessage::user("User", None))
            .add_message(ContextMessage::assistant("Assistant", None));

        let actual = fixture.message_groups(None);

        // Each message should be in its own group
        assert_eq!(actual.len(), 3);
        assert_eq!(actual[0].len(), 1);
        assert_eq!(actual[1].len(), 1);
        assert_eq!(actual[2].len(), 1);
    }

    #[test]
    fn test_message_iter_multiple_tool_calls() {
        use crate::{ToolCallFull, ToolCallId, ToolName, ToolOutput, ToolResult};

        let fixture = Context::default()
            .add_message(ContextMessage::system("System message"))
            .add_message(ContextMessage::assistant(
                "First tool call",
                Some(vec![ToolCallFull {
                    call_id: Some(ToolCallId::new("call1")),
                    name: ToolName::new("tool1"),
                    arguments: serde_json::json!({}),
                }]),
            ))
            .add_tool_results(vec![ToolResult {
                name: ToolName::new("tool1"),
                call_id: Some(ToolCallId::new("call1")),
                output: ToolOutput::text("Result 1".to_string()),
            }])
            .add_message(ContextMessage::assistant(
                "Second tool call",
                Some(vec![ToolCallFull {
                    call_id: Some(ToolCallId::new("call2")),
                    name: ToolName::new("tool2"),
                    arguments: serde_json::json!({}),
                }]),
            ))
            .add_tool_results(vec![ToolResult {
                name: ToolName::new("tool2"),
                call_id: Some(ToolCallId::new("call2")),
                output: ToolOutput::text("Result 2".to_string()),
            }]);

        let actual = fixture.message_groups(Some(1));

        // Should have 2 groups, each with assistant + tool result
        assert_eq!(actual.len(), 2);
        assert_eq!(actual[0].len(), 2); // First assistant + result
        assert_eq!(actual[1].len(), 2); // Second assistant + result
    }

    #[test]
    fn test_message_iter_parallel_tool_calls() {
        use crate::{ToolCallFull, ToolCallId, ToolName, ToolOutput, ToolResult};

        let fixture = Context::default()
            .add_message(ContextMessage::system("System message"))
            .add_message(ContextMessage::assistant(
                "First tool call",
                Some(vec![
                    ToolCallFull {
                        call_id: Some(ToolCallId::new("call1")),
                        name: ToolName::new("tool1"),
                        arguments: serde_json::json!({}),
                    },
                    ToolCallFull {
                        call_id: Some(ToolCallId::new("call2")),
                        name: ToolName::new("tool2"),
                        arguments: serde_json::json!({}),
                    },
                ]),
            ))
            .add_tool_results(vec![ToolResult {
                name: ToolName::new("tool1"),
                call_id: Some(ToolCallId::new("call1")),
                output: ToolOutput::text("Result 1".to_string()),
            }])
            .add_tool_results(vec![ToolResult {
                name: ToolName::new("tool2"),
                call_id: Some(ToolCallId::new("call2")),
                output: ToolOutput::text("Result 2".to_string()),
            }]);

        let actual = fixture.message_groups(Some(1));

        // Should have 2 groups, each with assistant + tool result
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].len(), 3); // First assistant + result
    }

    #[test]
    fn test_message_iter_assistance_tool_call_but_no_result() {
        use crate::{ToolCallFull, ToolCallId, ToolName};

        let fixture = Context::default()
            .add_message(ContextMessage::system("System"))
            .add_message(ContextMessage::user("User", None))
            .add_message(ContextMessage::assistant(
                "First tool call",
                Some(vec![ToolCallFull {
                    call_id: Some(ToolCallId::new("call1")),
                    name: ToolName::new("tool1"),
                    arguments: serde_json::json!({}),
                }]),
            ));

        let actual = fixture.message_groups(Some(1));

        // Should have 2 groups, System and User and Assistant with tool call won't be
        // in group as it's not completed yet.
        assert_eq!(actual.len(), 1);
    }

    #[test]
    fn test_count_tokens_text_messages() {
        // Test user message
        let fixture = ContextMessage::user("Hello world", None);
        let actual = fixture.count_tokens();
        let expected = 3; // "Hello world" = 11 chars, 11/4 = 2.75, ceil = 3
        assert_eq!(actual, expected);

        // Test assistant message
        let fixture = ContextMessage::assistant("How can I help you?", None);
        let actual = fixture.count_tokens();
        let expected = 5; // "How can I help you?" = 19 chars, 19/4 = 4.75, ceil = 5
        assert_eq!(actual, expected);

        // Test system message (not counted)
        let fixture = ContextMessage::system("You are a helpful assistant");
        let actual = fixture.count_tokens();
        let expected = 0; // System messages are not counted
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_count_tokens_text_message_with_tool_calls() {
        use crate::{ToolCallFull, ToolCallId, ToolName};

        let tool_call1 = ToolCallFull {
            call_id: Some(ToolCallId::new("call1")),
            name: ToolName::new("tool1"),           // 5 chars
            arguments: serde_json::json!({"a": 1}), // {"a":1} = 7 chars
        };

        let tool_call2 = ToolCallFull {
            call_id: Some(ToolCallId::new("call2")),
            name: ToolName::new("tool2"),           // 5 chars
            arguments: serde_json::json!({"b": 2}), // {"b":2} = 7 chars
        };

        let fixture =
            ContextMessage::assistant("Multiple tools", Some(vec![tool_call1, tool_call2]));
        let actual = fixture.count_tokens();

        // "Multiple tools" = 14 chars
        // "tool1" = 5 chars, {"a":1} = 7 chars
        // "tool2" = 5 chars, {"b":2} = 7 chars
        // Total: 14 + 5 + 7 + 5 + 7 = 38 chars, 38/4 = 9.5, ceil = 10
        let expected = 10;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_count_tokens_tool_result() {
        use crate::{ToolCallId, ToolName, ToolOutput, ToolResult, ToolValue};

        let fixture = ContextMessage::Tool(ToolResult {
            name: ToolName::new("test_tool"),
            call_id: Some(ToolCallId::new("call1")),
            output: ToolOutput {
                values: vec![
                    ToolValue::Text("First text".to_string()),  // 10 chars
                    ToolValue::Text("Second text".to_string()), // 11 chars
                ],
                is_error: false,
            },
        });

        let actual = fixture.count_tokens();
        let expected = 6; // 10 + 11 = 21 chars, 21/4 = 5.25, ceil = 6
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_count_tokens_image_message() {
        use crate::Image;

        let image = Image::new_base64("base64data".to_string(), "image/png");
        let fixture = ContextMessage::Image(image);

        let actual = fixture.count_tokens();
        let expected = 0; // Images are not counted
        assert_eq!(actual, expected);
    }
}
