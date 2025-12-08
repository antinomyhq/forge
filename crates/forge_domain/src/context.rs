use std::fmt::Display;
use std::ops::Deref;

use derive_more::derive::{Display, From};
use derive_setters::Setters;
use forge_template::Element;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::{ToolCallFull, ToolResult};

/// Helper function for serde to skip serializing false boolean values
fn is_false(value: &bool) -> bool {
    !value
}

use crate::temperature::Temperature;
use crate::top_k::TopK;
use crate::top_p::TopP;
use crate::{
    Attachment, AttachmentContent, ConversationId, EventValue, Image, ModelId, ReasoningFull,
    ToolChoice, ToolDefinition, ToolOutput, ToolValue, Usage,
};

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

/// Creates a filtered version of ToolOutput that excludes base64 images to
/// avoid serializing large image data in the context output
fn filter_base64_images_from_tool_output(output: &ToolOutput) -> ToolOutput {
    let filtered_values: Vec<ToolValue> = output
        .values
        .iter()
        .map(|value| match value {
            ToolValue::Image(image) => {
                // Skip base64 images (URLs that start with "data:")
                if image.url().starts_with("data:") {
                    ToolValue::Text(format!("[base64 image: {}]", image.mime_type()))
                } else {
                    value.clone()
                }
            }
            _ => value.clone(),
        })
        .collect();

    ToolOutput { is_error: output.is_error, values: filtered_values }
}

impl ContextMessage {
    pub fn content(&self) -> Option<&str> {
        match self {
            ContextMessage::Text(text_message) => Some(&text_message.content),
            ContextMessage::Tool(_) => None,
            ContextMessage::Image(_) => None,
        }
    }

    /// Returns the raw content before template rendering (only for User
    /// messages)
    pub fn as_value(&self) -> Option<&EventValue> {
        match self {
            ContextMessage::Text(text_message) => text_message.raw_content.as_ref(),
            ContextMessage::Tool(_) => None,
            ContextMessage::Image(_) => None,
        }
    }

    /// Estimates the number of tokens in a message using character-based
    /// approximation.
    /// ref: https://github.com/openai/codex/blob/main/codex-cli/src/utils/approximate-tokens-used.ts
    pub fn token_count_approx(&self) -> usize {
        let char_count = match self {
            ContextMessage::Text(text_message)
                if matches!(text_message.role, Role::User | Role::Assistant) =>
            {
                text_message.content.chars().count()
                    + tool_call_content_char_count(text_message)
                    + reasoning_content_char_count(text_message)
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

        char_count.div_ceil(4)
    }

    pub fn to_text(&self) -> String {
        match self {
            ContextMessage::Text(message) => {
                let mut message_element = Element::new("message").attr("role", message.role);

                message_element =
                    message_element.append(Element::new("content").text(&message.content));

                if let Some(tool_calls) = &message.tool_calls {
                    for call in tool_calls {
                        message_element = message_element.append(
                            Element::new("forge_tool_call")
                                .attr("name", &call.name)
                                .cdata(call.arguments.clone().into_string()),
                        );
                    }
                }

                if let Some(reasoning_details) = &message.reasoning_details {
                    for reasoning_detail in reasoning_details {
                        if let Some(text) = &reasoning_detail.text {
                            message_element =
                                message_element.append(Element::new("reasoning_detail").text(text));
                        }
                    }
                }

                message_element.render()
            }
            ContextMessage::Tool(result) => {
                let filtered_output = filter_base64_images_from_tool_output(&result.output);
                Element::new("message")
                    .attr("role", "tool")
                    .append(
                        Element::new("forge_tool_result")
                            .attr("name", &result.name)
                            .cdata(serde_json::to_string(&filtered_output).unwrap()),
                    )
                    .render()
            }
            ContextMessage::Image(_) => Element::new("image").attr("path", "[base64 URL]").render(),
        }
    }

    pub fn user(content: impl ToString, model: Option<ModelId>) -> Self {
        TextMessage {
            role: Role::User,
            content: content.to_string(),
            raw_content: None,
            tool_calls: None,
            reasoning_details: None,
            model,
            droppable: false,
        }
        .into()
    }

    pub fn system(content: impl ToString) -> Self {
        TextMessage {
            role: Role::System,
            content: content.to_string(),
            raw_content: None,
            tool_calls: None,
            model: None,
            reasoning_details: None,
            droppable: false,
        }
        .into()
    }

    pub fn assistant(
        content: impl ToString,
        reasoning_details: Option<Vec<ReasoningFull>>,
        tool_calls: Option<Vec<ToolCallFull>>,
    ) -> Self {
        let tool_calls =
            tool_calls.and_then(|calls| if calls.is_empty() { None } else { Some(calls) });
        TextMessage {
            role: Role::Assistant,
            content: content.to_string(),
            raw_content: None,
            tool_calls,
            reasoning_details,
            model: None,
            droppable: false,
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

    pub fn is_droppable(&self) -> bool {
        match self {
            ContextMessage::Text(message) => message.droppable,
            ContextMessage::Tool(_) => false,
            ContextMessage::Image(_) => false,
        }
    }

    pub fn has_tool_result(&self) -> bool {
        match self {
            ContextMessage::Text(_) => false,
            ContextMessage::Tool(_) => true,
            ContextMessage::Image(_) => false,
        }
    }

    pub fn has_tool_call(&self) -> bool {
        match self {
            ContextMessage::Text(message) => message.tool_calls.is_some(),
            ContextMessage::Tool(_) => false,
            ContextMessage::Image(_) => false,
        }
    }

    pub fn has_reasoning_details(&self) -> bool {
        match self {
            ContextMessage::Text(message) => message.reasoning_details.is_some(),
            ContextMessage::Tool(_) => false,
            ContextMessage::Image(_) => false,
        }
    }
}

fn tool_call_content_char_count(text_message: &TextMessage) -> usize {
    text_message
        .tool_calls
        .as_ref()
        .map(|tool_calls| {
            tool_calls
                .iter()
                .map(|tc| {
                    tc.arguments.to_owned().into_string().chars().count()
                        + tc.name.as_str().chars().count()
                })
                .sum()
        })
        .unwrap_or(0)
}

fn reasoning_content_char_count(text_message: &TextMessage) -> usize {
    text_message
        .reasoning_details
        .as_ref()
        .map_or(0, |details| {
            details
                .iter()
                .map(|rd| rd.text.as_ref().map_or(0, |text| text.chars().count()))
                .sum::<usize>()
        })
}

//TODO: Rename to TextMessage
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Setters)]
#[setters(strip_option, into)]
#[serde(rename_all = "snake_case")]
pub struct TextMessage {
    pub role: Role,
    pub content: String,
    /// The raw content before any template rendering (only for User messages)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_content: Option<EventValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallFull>>,
    // note: this used to track model used for this message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_details: Option<Vec<ReasoningFull>>,
    /// Indicates whether this message can be dropped during context compaction
    #[serde(default, skip_serializing_if = "is_false")]
    pub droppable: bool,
}

impl TextMessage {
    /// Creates a new TextMessage with the given role and content
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            raw_content: None,
            tool_calls: None,
            model: None,
            reasoning_details: None,
            droppable: false,
        }
    }

    pub fn has_role(&self, role: Role) -> bool {
        self.role == role
    }

    pub fn assistant(
        content: impl ToString,
        reasoning_details: Option<Vec<ReasoningFull>>,
        model: Option<ModelId>,
    ) -> Self {
        Self {
            role: Role::Assistant,
            content: content.to_string(),
            raw_content: None,
            tool_calls: None,
            reasoning_details,
            model,
            droppable: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize, Display)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<crate::agent_definition::ReasoningConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    /// Controls whether responses should be streamed. When `true`, responses
    /// are delivered incrementally as they're generated. When `false`, the
    /// complete response is returned at once. Defaults to `true` if not
    /// specified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

impl Context {
    pub fn system_prompt(&self) -> Option<&str> {
        self.messages
            .iter()
            .find(|message| message.has_role(Role::System))
            .and_then(|msg| msg.content())
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

    pub fn add_attachments(self, attachments: Vec<Attachment>, model_id: Option<ModelId>) -> Self {
        attachments.into_iter().fold(self, |ctx, attachment| {
            ctx.add_message(match attachment.content {
                AttachmentContent::Image(image) => ContextMessage::Image(image),
                AttachmentContent::FileContent { content, start_line, end_line, total_lines } => {
                    let elm = Element::new("file_content")
                        .attr("path", attachment.path)
                        .attr("start_line", start_line)
                        .attr("end_line", end_line)
                        .attr("total_lines", total_lines)
                        .cdata(content);

                    let mut message = TextMessage::new(Role::User, elm.to_string()).droppable(true);

                    if let Some(model) = model_id.clone() {
                        message = message.model(model);
                    }

                    message.into()
                }
                AttachmentContent::DirectoryListing { entries } => {
                    let elm = Element::new("directory_listing")
                        .attr("path", attachment.path)
                        .append(entries.into_iter().map(|entry| {
                            let tag_name = if entry.is_dir { "dir" } else { "file" };
                            Element::new(tag_name).text(entry.path)
                        }));

                    let mut message = TextMessage::new(Role::User, elm.to_string()).droppable(true);

                    if let Some(model) = model_id.clone() {
                        message = message.model(model);
                    }

                    message.into()
                }
            })
        })
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
    pub fn set_system_messages<S: Into<String>>(mut self, content: Vec<S>) -> Self {
        if self.messages.is_empty() {
            for message in content {
                self.messages.push(ContextMessage::system(message.into()));
            }
            self
        } else {
            // drop all the system messages;
            self.messages.retain(|m| !m.has_role(Role::System));
            // add the system message at the beginning.
            for message in content.into_iter().rev() {
                self.messages
                    .insert(0, ContextMessage::system(message.into()));
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
        reasoning_details: Option<Vec<ReasoningFull>>,
        tool_records: Vec<(ToolCallFull, ToolResult)>,
    ) -> Self {
        // Adding tool calls
        self.add_message(ContextMessage::assistant(
            content,
            reasoning_details,
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

    /// Returns the token count for context
    pub fn token_count(&self) -> TokenCount {
        let actual = self
            .usage
            .as_ref()
            .map(|u| u.total_tokens)
            .unwrap_or_default();

        match actual {
            TokenCount::Actual(actual) if actual > 0 => TokenCount::Actual(actual),
            _ => TokenCount::Approx(self.token_count_approx()),
        }
    }

    pub fn token_count_approx(&self) -> usize {
        self.messages
            .iter()
            .map(|m| m.token_count_approx())
            .sum::<usize>()
    }

    /// Checks if reasoning is enabled by user or not.
    pub fn is_reasoning_supported(&self) -> bool {
        self.reasoning.as_ref().is_some_and(|reasoning| {
            // When enabled parameter is defined then return it's value directly.
            if reasoning.enabled.is_some() {
                return reasoning.enabled.unwrap_or_default();
            }

            // If not defined (None), check other parameters
            reasoning.effort.is_some() || reasoning.max_tokens.is_some_and(|token| token > 0)
        })
    }

    /// Returns a vector of user messages, selecting the first message from
    /// each consecutive sequence of user messages.
    pub fn first_user_messages(&self) -> Vec<&ContextMessage> {
        if self.messages.is_empty() {
            return Vec::new();
        }

        let mut result = Vec::new();
        let mut is_user = false;

        for msg in &self.messages {
            if msg.has_role(Role::User) {
                // Only add the first message of each consecutive user sequence
                if !is_user {
                    result.push(msg);
                    is_user = true;
                }
            } else {
                is_user = false;
            }
        }

        result
    }

    /// Returns the total number of messages in the context
    pub fn total_messages(&self) -> usize {
        self.messages.len()
    }

    /// Returns the count of user messages in the context
    pub fn user_message_count(&self) -> usize {
        self.messages
            .iter()
            .filter(|msg| msg.has_role(Role::User))
            .count()
    }

    /// Returns the count of assistant messages in the context
    pub fn assistant_message_count(&self) -> usize {
        self.messages
            .iter()
            .filter(|msg| msg.has_role(Role::Assistant))
            .count()
    }

    /// Returns the total count of tool calls across all messages
    pub fn tool_call_count(&self) -> usize {
        self.messages
            .iter()
            .filter(|msg| msg.has_tool_call())
            .map(|msg| {
                if let ContextMessage::Text(text_msg) = msg {
                    text_msg.tool_calls.as_ref().map_or(0, |calls| calls.len())
                } else {
                    0
                }
            })
            .sum()
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum TokenCount {
    Actual(usize),
    Approx(usize),
}

impl Display for TokenCount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenCount::Actual(count) => write!(f, "{count}"),
            TokenCount::Approx(count) => write!(f, "~{count}"),
        }
    }
}

impl std::ops::Add for TokenCount {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        match (self, other) {
            (TokenCount::Actual(a), TokenCount::Actual(b)) => TokenCount::Actual(a + b),
            (TokenCount::Approx(a), TokenCount::Approx(b)) => TokenCount::Approx(a + b),
            (TokenCount::Actual(a), TokenCount::Approx(b)) => TokenCount::Approx(a + b),
            (TokenCount::Approx(a), TokenCount::Actual(b)) => TokenCount::Approx(a + b),
        }
    }
}

impl Default for TokenCount {
    fn default() -> Self {
        TokenCount::Actual(0)
    }
}

impl Deref for TokenCount {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        match self {
            TokenCount::Actual(i) => i,
            TokenCount::Approx(i) => i,
        }
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_yaml_snapshot;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::transformer::Transformer;
    use crate::{DirectoryEntry, estimate_token_count};

    #[test]
    fn test_override_system_message() {
        let request = Context::default()
            .add_message(ContextMessage::system("Initial system message"))
            .set_system_messages(vec!["Updated system message"]);

        assert_eq!(
            request.messages[0],
            ContextMessage::system("Updated system message"),
        );
    }

    #[test]
    fn test_set_system_message() {
        let request = Context::default().set_system_messages(vec!["A system message"]);

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
            .set_system_messages(vec!["A system message"]);

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
            .add_message(ContextMessage::assistant("Assistant message", None, None));

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
            .add_message(ContextMessage::assistant("Assistant message", None, None));
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
                    conversation_id: None,
                },
                ToolResult {
                    name: crate::ToolName::new("empty_tool"),
                    call_id: Some(crate::ToolCallId::new("call2")),
                    output: crate::ToolOutput {
                        values: vec![crate::ToolValue::Empty],
                        is_error: false,
                    },
                    conversation_id: None,
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
                conversation_id: None,
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
            conversation_id: None,
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
                    conversation_id: None,
                },
                ToolResult {
                    name: crate::ToolName::new("image_tool1"),
                    call_id: Some(crate::ToolCallId::new("call2")),
                    output: crate::ToolOutput::image(image1),
                    conversation_id: None,
                },
                ToolResult {
                    name: crate::ToolName::new("image_tool2"),
                    call_id: Some(crate::ToolCallId::new("call3")),
                    output: crate::ToolOutput::image(image2),
                    conversation_id: None,
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
            .add_message(ContextMessage::assistant("Assistant response", None, None))
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
                conversation_id: None,
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
            conversation_id: None,
        }]);

        let mut transformer = crate::transformer::ImageHandling::new();
        let actual = transformer.transform(fixture);

        assert_yaml_snapshot!(actual);
    }

    #[test]
    fn test_context_should_return_max_token_count() {
        let fixture = Context::default();
        let actual = fixture.token_count();
        let expected = TokenCount::Approx(0); // Empty context has no tokens
        assert_eq!(actual, expected);

        // case 2: context with usage - since total_tokens present return that.
        let usage = Usage { total_tokens: TokenCount::Actual(100), ..Default::default() };
        let fixture = Context::default().usage(usage);
        assert_eq!(fixture.token_count(), TokenCount::Actual(100));

        // case 3: context with usage - since total_tokens present return that.
        let usage = Usage { total_tokens: TokenCount::Actual(80), ..Default::default() };
        let fixture = Context::default().usage(usage);
        assert_eq!(fixture.token_count(), TokenCount::Actual(80));

        // case 4: context with messages - since total_tokens are not present return
        // estimate
        let usage = Usage::default();
        let fixture = Context::default()
            .add_message(ContextMessage::user("Hello", None))
            .add_message(ContextMessage::assistant("Hi there!", None, None))
            .add_message(ContextMessage::assistant("How can I help you?", None, None))
            .add_message(ContextMessage::user("I'm looking for a restaurant.", None))
            .usage(usage);
        assert_eq!(fixture.token_count(), TokenCount::Approx(18));
    }

    #[test]
    fn test_context_is_reasoning_supported_when_enabled() {
        let fixture = Context::default().reasoning(crate::agent_definition::ReasoningConfig {
            enabled: Some(true),
            ..Default::default()
        });

        let actual = fixture.is_reasoning_supported();
        let expected = true;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_is_reasoning_supported_when_effort_set() {
        let fixture = Context::default().reasoning(crate::agent_definition::ReasoningConfig {
            effort: Some(crate::agent_definition::Effort::High),
            ..Default::default()
        });

        let actual = fixture.is_reasoning_supported();
        let expected = true;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_is_reasoning_supported_when_max_tokens_positive() {
        let fixture = Context::default().reasoning(crate::agent_definition::ReasoningConfig {
            max_tokens: Some(1024),
            ..Default::default()
        });

        let actual = fixture.is_reasoning_supported();
        let expected = true;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_is_reasoning_not_supported_when_max_tokens_zero() {
        let fixture = Context::default().reasoning(crate::agent_definition::ReasoningConfig {
            max_tokens: Some(0),
            ..Default::default()
        });

        let actual = fixture.is_reasoning_supported();
        let expected = false;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_is_reasoning_not_supported_when_disabled() {
        let fixture = Context::default().reasoning(crate::agent_definition::ReasoningConfig {
            enabled: Some(false),
            ..Default::default()
        });

        let actual = fixture.is_reasoning_supported();
        let expected = false;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_is_reasoning_not_supported_when_no_config() {
        let fixture = Context::default();

        let actual = fixture.is_reasoning_supported();
        let expected = false;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_is_reasoning_not_supported_when_explicitly_disabled() {
        let fixture = Context::default().reasoning(crate::agent_definition::ReasoningConfig {
            enabled: Some(false),
            effort: Some(crate::agent_definition::Effort::High), /* Should be ignored when
                                                                  * explicitly disabled */
            ..Default::default()
        });

        let actual = fixture.is_reasoning_supported();
        let expected = false;

        assert_eq!(
            actual, expected,
            "Should not be supported when explicitly disabled, even with effort set"
        );
    }

    #[test]
    fn test_add_attachments_file_content_is_droppable() {
        let fixture_attachments = vec![Attachment {
            path: "/path/to/file.rs".to_string(),
            content: AttachmentContent::FileContent {
                content: "fn main() {}\n".to_string(),
                start_line: 1,
                end_line: 1,
                total_lines: 1,
            },
        }];

        let fixture_model = ModelId::new("test-model");
        let actual = Context::default().add_attachments(fixture_attachments, Some(fixture_model));

        // Verify the message was added
        assert_eq!(actual.messages.len(), 1);

        // Verify the message is droppable
        let message = &actual.messages[0];
        assert!(
            message.is_droppable(),
            "File content attachments should be marked as droppable"
        );

        // Verify the message is a User message
        assert!(message.has_role(Role::User));
    }

    #[test]
    fn test_add_attachments_image_is_not_droppable() {
        let fixture_image = Image::new_base64("base64data".to_string(), "image/png");
        let fixture_attachments = vec![Attachment {
            path: "image.png".to_string(),
            content: AttachmentContent::Image(fixture_image),
        }];

        let actual = Context::default().add_attachments(fixture_attachments, None);

        // Verify the message was added
        assert_eq!(actual.messages.len(), 1);

        // Verify the image message is NOT droppable (images use different
        // ContextMessage variant)
        let message = &actual.messages[0];
        assert!(
            !message.is_droppable(),
            "Image attachments should not be marked as droppable"
        );
    }

    #[test]
    fn test_add_attachments_multiple_file_contents_all_droppable() {
        let fixture_attachments = vec![
            Attachment {
                path: "/path/to/file1.rs".to_string(),
                content: AttachmentContent::FileContent {
                    content: "fn foo() {}\n".to_string(),
                    start_line: 1,
                    end_line: 1,
                    total_lines: 1,
                },
            },
            Attachment {
                path: "/path/to/file2.rs".to_string(),
                content: AttachmentContent::FileContent {
                    content: "fn bar() {}\n".to_string(),
                    start_line: 1,
                    end_line: 1,
                    total_lines: 1,
                },
            },
        ];

        let actual = Context::default().add_attachments(fixture_attachments, None);

        // Verify both messages were added
        assert_eq!(actual.messages.len(), 2);

        // Verify all file content messages are droppable
        for message in &actual.messages {
            assert!(
                message.is_droppable(),
                "All file content attachments should be marked as droppable"
            );
        }
    }

    #[test]
    fn test_add_attachments_directory_listing() {
        let fixture_attachments = vec![Attachment {
            path: "/test/mydir".to_string(),
            content: AttachmentContent::DirectoryListing {
                entries: vec![
                    DirectoryEntry { path: "/test/mydir/file1.txt".to_string(), is_dir: false },
                    DirectoryEntry { path: "/test/mydir/file2.rs".to_string(), is_dir: false },
                    DirectoryEntry { path: "/test/mydir/subdir".to_string(), is_dir: true },
                ],
            },
        }];

        let actual = Context::default().add_attachments(fixture_attachments, None);

        // Verify message was added
        assert_eq!(actual.messages.len(), 1);

        // Verify directory listing is formatted correctly as XML
        let message = actual.messages.first().unwrap();
        assert!(
            message.is_droppable(),
            "Directory listing should be marked as droppable"
        );

        let text = message.to_text();
        // The XML is encoded within the message content
        assert!(text.contains("&lt;directory_listing"));
        // Check that files use <file> tag
        assert!(text.contains("&lt;file&gt;"));
        // Check that directories use <dir> tag
        assert!(text.contains("&lt;dir&gt;"));
    }

    #[test]
    fn test_context_message_statistics() {
        let fixture = Context::default()
            .add_message(ContextMessage::system("System message"))
            .add_message(ContextMessage::user("User message 1", None))
            .add_message(ContextMessage::assistant("Assistant response", None, None))
            .add_message(ContextMessage::user("User message 2", None))
            .add_message(ContextMessage::assistant(
                "Assistant with tool",
                None,
                Some(vec![
                    ToolCallFull {
                        call_id: Some(crate::ToolCallId::new("call1")),
                        name: crate::ToolName::new("tool1"),
                        arguments: serde_json::json!({"arg": "value"}).into(),
                    },
                    ToolCallFull {
                        call_id: Some(crate::ToolCallId::new("call2")),
                        name: crate::ToolName::new("tool2"),
                        arguments: serde_json::json!({"arg": "value"}).into(),
                    },
                ]),
            ))
            .add_tool_results(vec![
                ToolResult {
                    name: crate::ToolName::new("tool1"),
                    call_id: Some(crate::ToolCallId::new("call1")),
                    output: crate::ToolOutput::text("Result 1".to_string()),
                    conversation_id: None,
                },
                ToolResult {
                    name: crate::ToolName::new("tool2"),
                    call_id: Some(crate::ToolCallId::new("call2")),
                    output: crate::ToolOutput::text("Result 2".to_string()),
                    conversation_id: None,
                },
            ]);

        // Test total messages (6 messages: 1 system + 2 user + 2 assistant + 2 tool
        // results)
        assert_eq!(fixture.total_messages(), 7);

        // Test user message count
        assert_eq!(fixture.user_message_count(), 2);

        // Test assistant message count
        assert_eq!(fixture.assistant_message_count(), 2);

        // Test tool call count (2 tool calls in the second assistant message)
        assert_eq!(fixture.tool_call_count(), 2);
    }

    #[test]
    fn test_directory_listing_sorted_dirs_first() {
        // Create entries already sorted (as they would come from attachment service)
        // Directories first, then files, all sorted alphabetically
        let fixture_attachments = vec![Attachment {
            path: "/test/root".to_string(),
            content: AttachmentContent::DirectoryListing {
                entries: vec![
                    DirectoryEntry { path: "apple_dir".to_string(), is_dir: true },
                    DirectoryEntry { path: "berry_dir".to_string(), is_dir: true },
                    DirectoryEntry { path: "zoo_dir".to_string(), is_dir: true },
                    DirectoryEntry { path: "banana.txt".to_string(), is_dir: false },
                    DirectoryEntry { path: "cherry.txt".to_string(), is_dir: false },
                    DirectoryEntry { path: "zebra.txt".to_string(), is_dir: false },
                ],
            },
        }];

        let actual = Context::default().add_attachments(fixture_attachments, None);
        let text = actual.messages.first().unwrap().to_text();

        // Extract the order of entries from the XML
        let dir_entries: Vec<&str> = text
            .split("&lt;")
            .filter(|s| s.starts_with("dir&gt;") || s.starts_with("file&gt;"))
            .collect();

        // Verify directories come first, then files, all sorted alphabetically
        let expected_order = [
            "dir&gt;apple_dir",
            "dir&gt;berry_dir",
            "dir&gt;zoo_dir",
            "file&gt;banana.txt",
            "file&gt;cherry.txt",
            "file&gt;zebra.txt",
        ];

        for (i, expected) in expected_order.iter().enumerate() {
            assert!(
                dir_entries[i].starts_with(expected),
                "Expected entry {} to start with '{}', but got '{}'",
                i,
                expected,
                dir_entries[i]
            );
        }
    }
}
