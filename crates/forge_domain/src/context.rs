use std::fmt::Display;
use std::ops::Deref;

use derive_more::derive::{Display, From};
use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::{ToolCallFull, ToolResult};
use crate::temperature::Temperature;
use crate::top_k::TopK;
use crate::top_p::TopP;
use crate::{
    ConversationId, Image, ModelId, ReasoningFull, ToolChoice, ToolDefinition, ToolValue, Usage,
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

impl ContextMessage {
    pub fn content(&self) -> Option<&str> {
        match self {
            ContextMessage::Text(text_message) => Some(&text_message.content),
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
                if let Some(reasoning_details) = &message.reasoning_details {
                    for reasoning_detail in reasoning_details {
                        if let Some(text) = &reasoning_detail.text {
                            lines.push_str(&format!("<reasoning_detail>{text}</reasoning_detail>"));
                        }
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
            reasoning_details: None,
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
            reasoning_details: None,
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
            tool_calls,
            reasoning_details,
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
                    tc.arguments.to_string().chars().count() + tc.name.as_str().chars().count()
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallFull>>,
    // note: this used to track model used for this message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_details: Option<Vec<ReasoningFull>>,
}

impl TextMessage {
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
            tool_calls: None,
            reasoning_details,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<crate::agent::ReasoningConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
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
            .map(|u| u.total_tokens.clone())
            .unwrap_or_default();

        match actual {
            TokenCount::Actual(actual) if actual > 0 => TokenCount::Actual(actual),
            _ => TokenCount::Approx(
                self.messages
                    .iter()
                    .map(|m| m.token_count_approx())
                    .sum::<usize>(),
            ),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
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
        TokenCount::Approx(0)
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
    fn test_context_should_return_max_token_count() {
        let fixture = Context::default();
        let actual = fixture.token_count();
        let expected = TokenCount::Approx(0); // Empty context has no tokens
        assert_eq!(actual, expected);

        // case 2: context with usage - since total_tokens present return that.
        let mut usage = Usage::default();
        usage.total_tokens = TokenCount::Actual(100);
        let fixture = Context::default().usage(usage);
        assert_eq!(fixture.token_count(), TokenCount::Actual(100));

        // case 3: context with usage - since total_tokens present return that.
        let mut usage = Usage::default();
        usage.total_tokens = TokenCount::Actual(80);
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
}
