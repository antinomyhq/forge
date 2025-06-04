use derive_more::derive::{Display, From};
use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::{Pdf, ToolCallFull, ToolOutputValue, ToolResult};
use crate::temperature::Temperature;
use crate::top_k::TopK;
use crate::top_p::TopP;
use crate::{Image, ModelId, ToolChoice, ToolDefinition};

/// Represents a message being sent to the LLM provider
/// NOTE: ToolResults message are part of the larger Request object and not part
/// of the message.
#[derive(Clone, Debug, Deserialize, From, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ContextMessage {
    Text(TextMessage),
    Tool(ToolResult),
    Image(Image),
    Pdf(Pdf),
}

impl ContextMessage {
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
            ContextMessage::Image(_) | ContextMessage::Pdf(_) => Role::User == role,
        }
    }

    pub fn has_tool_call(&self) -> bool {
        match self {
            ContextMessage::Text(message) => message.tool_calls.is_some(),
            ContextMessage::Tool(_) => false,
            ContextMessage::Image(_) => false,
            ContextMessage::Pdf(_) => false,
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
    pub fn add_pdf(mut self, pdf: Pdf) -> Self {
        self.messages.push(ContextMessage::Pdf(pdf));
        self
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
            match message {
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
                ContextMessage::Pdf(_) => {
                    lines.push_str("<pdf path=\"[base64 URL]\">".to_string().as_str());
                }
            }
        }

        format!("<chat_history>{lines}</chat_history>")
    }

    /// Will append a message to the context. If the model supports tools, it
    /// will append the tool calls and results to the message. If the model
    /// does not support tools, it will append the tool calls and results as
    /// separate messages.
    pub fn append_message(
        mut self,
        content: impl ToString,
        model: ModelId,
        tool_records: Vec<(ToolCallFull, ToolResult)>,
        tool_supported: bool,
    ) -> Self {
        if tool_supported {
            // Adding tool calls
            let context = self
                .add_message(ContextMessage::assistant(
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
                );

            update_image_tool_calls(context)
        } else {
            // Adding tool calls
            self = self.add_message(ContextMessage::assistant(content.to_string(), None));
            if tool_records.is_empty() {
                return self;
            }

            // Adding tool results as user message
            let outputs = tool_records
                .iter()
                .flat_map(|record| record.1.output.values.iter());
            for out in outputs {
                match out {
                    crate::ToolOutputValue::Text(text) => {
                        self = self.add_message(ContextMessage::user(text, Some(model.clone())));
                    }
                    crate::ToolOutputValue::Image(base64_url) => {
                        self = self.add_base64_url(base64_url.clone());
                    }
                    ToolOutputValue::Pdf(pdf) => {
                        self = self.add_pdf(pdf.clone());
                    }
                    crate::ToolOutputValue::Empty => {}
                }
            }
            self
        }
    }
}

// Assuming tool calls are supported by the model we will convert all tool
// results that contain images into user messages
fn update_image_tool_calls(mut context: Context) -> Context {
    let mut images = Vec::new();
    let mut pdfs = Vec::new();

    // Step 1: Replace the image value with a text message
    context
        .messages
        .iter_mut()
        .filter_map(|message| {
            if let ContextMessage::Tool(tool_result) = message {
                Some(tool_result)
            } else {
                None
            }
        })
        .flat_map(|tool_result| tool_result.output.values.iter_mut())
        .for_each(|value| match value {
            crate::ToolOutputValue::Image(image) => {
                let image = std::mem::take(image);
                let id = images.len();
                *value = crate::ToolOutputValue::Text(format!(
                    "[The image with ID {id} will be sent as an attachment in the next message]"
                ));
                images.push((id, image));
            }
            ToolOutputValue::Pdf(pdf) => {
                let pdf = std::mem::take(pdf);
                let id = pdfs.len();
                *value = ToolOutputValue::Text(format!(
                    "[The pdf with ID {id} will be sent as an attachment in the next message]"
                ));
                pdfs.push((id, pdf));
            }
            crate::ToolOutputValue::Text(_) => {}
            crate::ToolOutputValue::Empty => {}
        });

    // Step 2: Insert all images in the end
    images.into_iter().for_each(|(id, image)| {
        context.messages.push(ContextMessage::user(
            format!("[Here is the image attachment for ID {id}]"),
            None,
        ));
        context.messages.push(ContextMessage::Image(image));
    });

    pdfs.into_iter().for_each(|(id, pdf)| {
        context.messages.push(ContextMessage::user(
            format!("[Here is the pdf for ID {id}]"),
            None,
        ));
        context.messages.push(ContextMessage::Pdf(pdf));
    });

    context
}

#[cfg(test)]
mod tests {
    use insta::assert_yaml_snapshot;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::estimate_token_count;

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
    fn test_append_message_with_tool_support_empty_tool_records() {
        let model = ModelId::new("test-model");
        let fixture = Context::default();

        let actual = fixture.append_message("Hello world", model.clone(), vec![], true);

        let expected =
            Context::default().add_message(ContextMessage::assistant("Hello world", None));

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_append_message_with_tool_support_single_tool_record() {
        let model = ModelId::new("test-model");
        let fixture = Context::default();

        let tool_call = ToolCallFull {
            name: crate::ToolName::new("test_tool"),
            call_id: Some(crate::ToolCallId::new("call123")),
            arguments: serde_json::json!({"param": "value"}),
        };

        let tool_result = ToolResult {
            name: crate::ToolName::new("test_tool"),
            call_id: Some(crate::ToolCallId::new("call123")),
            output: crate::ToolOutput::text("Tool output".to_string()),
        };

        let actual = fixture.append_message(
            "Hello world",
            model.clone(),
            vec![(tool_call.clone(), tool_result.clone())],
            true,
        );

        let expected = Context::default()
            .add_message(ContextMessage::assistant(
                "Hello world",
                Some(vec![tool_call]),
            ))
            .add_tool_results(vec![tool_result]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_append_message_with_tool_support_multiple_tool_records() {
        let model = ModelId::new("test-model");
        let fixture = Context::default();

        let tool_call1 = ToolCallFull {
            name: crate::ToolName::new("tool1"),
            call_id: Some(crate::ToolCallId::new("call1")),
            arguments: serde_json::json!({"param1": "value1"}),
        };

        let tool_call2 = ToolCallFull {
            name: crate::ToolName::new("tool2"),
            call_id: Some(crate::ToolCallId::new("call2")),
            arguments: serde_json::json!({"param2": "value2"}),
        };

        let tool_result1 = ToolResult {
            name: crate::ToolName::new("tool1"),
            call_id: Some(crate::ToolCallId::new("call1")),
            output: crate::ToolOutput::text("Tool 1 output".to_string()),
        };

        let tool_result2 = ToolResult {
            name: crate::ToolName::new("tool2"),
            call_id: Some(crate::ToolCallId::new("call2")),
            output: crate::ToolOutput::text("Tool 2 output".to_string()),
        };

        let actual = fixture.append_message(
            "Processing complete",
            model.clone(),
            vec![
                (tool_call1.clone(), tool_result1.clone()),
                (tool_call2.clone(), tool_result2.clone()),
            ],
            true,
        );

        let expected = Context::default()
            .add_message(ContextMessage::assistant(
                "Processing complete",
                Some(vec![tool_call1, tool_call2]),
            ))
            .add_tool_results(vec![tool_result1, tool_result2]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_append_message_without_tool_support_empty_tool_records() {
        let model = ModelId::new("test-model");
        let fixture = Context::default();

        let actual = fixture.append_message("Hello world", model.clone(), vec![], false);

        let expected =
            Context::default().add_message(ContextMessage::assistant("Hello world", None));

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_append_message_without_tool_support_single_text_output() {
        let model = ModelId::new("test-model");
        let fixture = Context::default();

        let tool_call = ToolCallFull {
            name: crate::ToolName::new("test_tool"),
            call_id: Some(crate::ToolCallId::new("call123")),
            arguments: serde_json::json!({"param": "value"}),
        };

        let tool_result = ToolResult {
            name: crate::ToolName::new("test_tool"),
            call_id: Some(crate::ToolCallId::new("call123")),
            output: crate::ToolOutput::text("Tool output".to_string()),
        };

        let actual = fixture.append_message(
            "Processing",
            model.clone(),
            vec![(tool_call, tool_result)],
            false,
        );

        let expected = Context::default()
            .add_message(ContextMessage::assistant("Processing", None))
            .add_message(ContextMessage::user("Tool output", Some(model)));

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_append_message_without_tool_support_single_image_output() {
        let model = ModelId::new("test-model");
        let fixture = Context::default();

        let image = Image::new_base64("test123".to_string(), "image/png");

        let tool_call = ToolCallFull {
            name: crate::ToolName::new("image_tool"),
            call_id: Some(crate::ToolCallId::new("call123")),
            arguments: serde_json::json!({"generate": "image"}),
        };

        let tool_result = ToolResult {
            name: crate::ToolName::new("image_tool"),
            call_id: Some(crate::ToolCallId::new("call123")),
            output: crate::ToolOutput::image(image.clone()),
        };

        let actual = fixture.append_message(
            "Image generated",
            model.clone(),
            vec![(tool_call, tool_result)],
            false,
        );

        let expected = Context::default()
            .add_message(ContextMessage::assistant("Image generated", None))
            .add_base64_url(image);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_append_message_without_tool_support_empty_output() {
        let model = ModelId::new("test-model");
        let fixture = Context::default();

        let tool_call = ToolCallFull {
            name: crate::ToolName::new("void_tool"),
            call_id: Some(crate::ToolCallId::new("call123")),
            arguments: serde_json::json!({}),
        };

        let tool_result = ToolResult {
            name: crate::ToolName::new("void_tool"),
            call_id: Some(crate::ToolCallId::new("call123")),
            output: crate::ToolOutput {
                values: vec![crate::ToolOutputValue::Empty],
                is_error: false,
            },
        };

        let actual = fixture.append_message(
            "Task completed",
            model.clone(),
            vec![(tool_call, tool_result)],
            false,
        );

        let expected =
            Context::default().add_message(ContextMessage::assistant("Task completed", None));

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_append_message_without_tool_support_mixed_outputs() {
        let model = ModelId::new("test-model");
        let fixture = Context::default();

        let image = Image::new_base64("test123".to_string(), "image/png");

        let tool_call1 = ToolCallFull {
            name: crate::ToolName::new("text_tool"),
            call_id: Some(crate::ToolCallId::new("call1")),
            arguments: serde_json::json!({"generate": "text"}),
        };

        let tool_result1 = ToolResult {
            name: crate::ToolName::new("text_tool"),
            call_id: Some(crate::ToolCallId::new("call1")),
            output: crate::ToolOutput::text("Text result".to_string()),
        };

        let tool_call2 = ToolCallFull {
            name: crate::ToolName::new("image_tool"),
            call_id: Some(crate::ToolCallId::new("call2")),
            arguments: serde_json::json!({"generate": "image"}),
        };

        let tool_result2 = ToolResult {
            name: crate::ToolName::new("image_tool"),
            call_id: Some(crate::ToolCallId::new("call2")),
            output: crate::ToolOutput::image(image.clone()),
        };

        let actual = fixture.append_message(
            "Mixed outputs generated",
            model.clone(),
            vec![(tool_call1, tool_result1), (tool_call2, tool_result2)],
            false,
        );

        let expected = Context::default()
            .add_message(ContextMessage::assistant("Mixed outputs generated", None))
            .add_message(ContextMessage::user("Text result", Some(model)))
            .add_base64_url(image);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_append_message_without_tool_support_multiple_values_in_single_output() {
        let model = ModelId::new("test-model");
        let fixture = Context::default();

        let image = Image::new_base64("test123".to_string(), "image/png");

        let tool_call = ToolCallFull {
            name: crate::ToolName::new("multi_tool"),
            call_id: Some(crate::ToolCallId::new("call1")),
            arguments: serde_json::json!({"multi": true}),
        };

        let tool_result = ToolResult {
            name: crate::ToolName::new("multi_tool"),
            call_id: Some(crate::ToolCallId::new("call1")),
            output: crate::ToolOutput {
                values: vec![
                    crate::ToolOutputValue::Text("First text".to_string()),
                    crate::ToolOutputValue::Image(image.clone()),
                    crate::ToolOutputValue::Text("Second text".to_string()),
                    crate::ToolOutputValue::Empty,
                ],
                is_error: false,
            },
        };

        let actual = fixture.append_message(
            "Multiple values generated",
            model.clone(),
            vec![(tool_call, tool_result)],
            false,
        );

        let expected = Context::default()
            .add_message(ContextMessage::assistant("Multiple values generated", None))
            .add_message(ContextMessage::user("First text", Some(model.clone())))
            .add_base64_url(image)
            .add_message(ContextMessage::user("Second text", Some(model)));

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_append_message_preserves_existing_context() {
        let model = ModelId::new("test-model");
        let fixture = Context::default()
            .add_message(ContextMessage::system("System prompt"))
            .add_message(ContextMessage::user("User question", Some(model.clone())));

        let tool_call = ToolCallFull {
            name: crate::ToolName::new("test_tool"),
            call_id: Some(crate::ToolCallId::new("call123")),
            arguments: serde_json::json!({"param": "value"}),
        };

        let tool_result = ToolResult {
            name: crate::ToolName::new("test_tool"),
            call_id: Some(crate::ToolCallId::new("call123")),
            output: crate::ToolOutput::text("Tool output".to_string()),
        };

        let actual = fixture.append_message(
            "Response with tool",
            model.clone(),
            vec![(tool_call.clone(), tool_result.clone())],
            true,
        );

        let expected = Context::default()
            .add_message(ContextMessage::system("System prompt"))
            .add_message(ContextMessage::user("User question", Some(model)))
            .add_message(ContextMessage::assistant(
                "Response with tool",
                Some(vec![tool_call]),
            ))
            .add_tool_results(vec![tool_result]);

        assert_eq!(actual, expected);
    }
    #[test]
    fn test_update_image_tool_calls_empty_context() {
        let fixture = Context::default();

        let actual = update_image_tool_calls(fixture);

        assert_yaml_snapshot!(actual);
    }

    #[test]
    fn test_update_image_tool_calls_no_tool_results() {
        let fixture = Context::default()
            .add_message(ContextMessage::system("System message"))
            .add_message(ContextMessage::user("User message", None))
            .add_message(ContextMessage::assistant("Assistant message", None));

        let actual = update_image_tool_calls(fixture);

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
                        values: vec![crate::ToolOutputValue::Empty],
                        is_error: false,
                    },
                },
            ]);

        let actual = update_image_tool_calls(fixture);

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

        let actual = update_image_tool_calls(fixture);

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
                    crate::ToolOutputValue::Text("First text".to_string()),
                    crate::ToolOutputValue::Image(image1),
                    crate::ToolOutputValue::Text("Second text".to_string()),
                    crate::ToolOutputValue::Image(image2),
                ],
                is_error: false,
            },
        }]);

        let actual = update_image_tool_calls(fixture);

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

        let actual = update_image_tool_calls(fixture);

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
                        crate::ToolOutputValue::Text("Before image".to_string()),
                        crate::ToolOutputValue::Image(image),
                        crate::ToolOutputValue::Text("After image".to_string()),
                        crate::ToolOutputValue::Empty,
                    ],
                    is_error: false,
                },
            }]);

        let actual = update_image_tool_calls(fixture);

        assert_yaml_snapshot!(actual);
    }

    #[test]
    fn test_update_image_tool_calls_preserves_error_flag() {
        let image = Image::new_base64("test123".to_string(), "image/png");
        let fixture = Context::default().add_tool_results(vec![ToolResult {
            name: crate::ToolName::new("error_tool"),
            call_id: Some(crate::ToolCallId::new("call1")),
            output: crate::ToolOutput {
                values: vec![crate::ToolOutputValue::Image(image)],
                is_error: true,
            },
        }]);

        let actual = update_image_tool_calls(fixture);

        assert_yaml_snapshot!(actual);
    }
}
