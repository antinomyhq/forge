use std::fmt::Display;
use std::ops::Deref;

use derive_more::derive::{Display, From};
use derive_setters::Setters;
use forge_template::Element;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::{ToolCallFull, ToolResult};
use crate::temperature::Temperature;
use crate::top_k::TopK;
use crate::top_p::TopP;
use crate::{
    ConversationId, Image, ModelId, ReasoningFull, ToolChoice, ToolDefinition, ToolOutput,
    ToolValue, Usage,
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
                let mut message_element = Element::new("message").attr("role", &message.role);

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
            original_content: None,
            tool_calls: None,
            reasoning_details: None,
            model,
        }
        .into()
    }

    /// Creates a user message with both original and formatted content
    pub fn user_with_original(
        original: impl ToString,
        formatted: impl ToString,
        model: Option<ModelId>,
    ) -> Self {
        TextMessage {
            role: Role::User,
            content: formatted.to_string(),
            original_content: Some(original.to_string()),
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
            original_content: None,
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
            original_content: None,
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
    /// Original unwrapped content before any transformations.
    /// Only populated for User messages where template wrapping is applied.
    /// This field is for internal use only (summaries, logging, UI).
    /// NOT sent to LLM APIs - the DTO conversion layer explicitly maps only the
    /// `content` field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_content: Option<String>,
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
            original_content: None,
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
            .map(|u| u.total_tokens.clone())
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

    /// Counts the total number of tool calls in messages within a given range.
    fn count_tool_calls_in_range(&self, start_index: usize, end_index: usize) -> usize {
        if start_index >= end_index {
            return 0;
        }

        self.messages
            .iter()
            .enumerate()
            .skip(start_index + 1)
            .take(end_index.saturating_sub(start_index + 1))
            .filter_map(|(_, message)| {
                if let ContextMessage::Text(text_message) = message
                    && text_message.role == Role::Assistant
                {
                    text_message.tool_calls.as_ref().map(|calls| calls.len())
                } else {
                    None
                }
            })
            .sum()
    }

    /// Finds the message with the specified role that appears before a given
    /// message index.
    pub fn find_message_with_role_before(&self, message_index: usize, role: Role) -> Option<&str> {
        for message in self.messages.iter().take(message_index).rev() {
            if let ContextMessage::Text(text_message) = message
                && text_message.role == role
            {
                return match role {
                    Role::User => Some(
                        text_message
                            .original_content
                            .as_deref()
                            .unwrap_or(&text_message.content),
                    ),
                    _ => Some(&text_message.content),
                };
            }
        }
        None
    }

    /// Find all user message indices in the conversation
    fn find_user_message_indices(&self) -> Vec<usize> {
        self.messages
            .iter()
            .enumerate()
            .filter_map(|(index, msg)| {
                if msg.has_role(Role::User) {
                    Some(index)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Find the last assistant message before a given index (exclusive)
    fn find_last_assistant_before(&self, before_index: usize) -> Option<usize> {
        self.messages
            .iter()
            .take(before_index)
            .enumerate()
            .rev()
            .find_map(|(index, msg)| {
                if msg.has_role(Role::Assistant) {
                    Some(index)
                } else {
                    None
                }
            })
    }

    /// Check if there's at least one assistant message between two indices
    fn has_assistant_between(&self, start: usize, end: usize) -> bool {
        if start >= end {
            return false;
        }

        self.messages
            .iter()
            .enumerate()
            .skip(start + 1)
            .take(end - start - 1)
            .any(|(_, msg)| msg.has_role(Role::Assistant))
    }

    /// Find the next user message index after a given index
    fn find_next_user_index(&self, after_index: usize) -> Option<usize> {
        self.messages
            .iter()
            .enumerate()
            .skip(after_index + 1)
            .find_map(|(index, msg)| {
                if msg.has_role(Role::User) {
                    Some(index)
                } else {
                    None
                }
            })
    }

    /// Checks if there's an assistant message after the given user index
    fn has_assistant_after(&self, user_index: usize) -> bool {
        self.messages
            .iter()
            .skip(user_index + 1)
            .any(|msg| msg.has_role(Role::Assistant))
    }

    /// Finds the last assistant message before the next user boundary (or end
    /// of conversation if no next user)
    fn find_last_assistant_in_range(&self, next_user_index: Option<usize>) -> Option<usize> {
        let boundary = next_user_index.unwrap_or(self.messages.len());
        self.find_last_assistant_before(boundary)
    }

    /// Extracts user message content at the given index
    fn extract_user_content(&self, index: usize) -> Option<&str> {
        self.messages.get(index).and_then(|msg| {
            if let ContextMessage::Text(text_message) = msg
                && text_message.role == Role::User
            {
                Some(
                    text_message
                        .original_content
                        .as_deref()
                        .unwrap_or(&text_message.content),
                )
            } else {
                None
            }
        })
    }

    /// Extracts assistant message content at the given index
    fn extract_assistant_content(&self, index: usize) -> Option<&str> {
        self.messages.get(index).and_then(|msg| {
            if let ContextMessage::Text(text_message) = msg
                && text_message.role == Role::Assistant
            {
                Some(text_message.content.as_str())
            } else {
                None
            }
        })
    }

    /// Creates a completion entry from user and assistant indices
    fn create_entry(
        &self,
        user_index: usize,
        assistant_index: usize,
    ) -> Option<crate::CompletionEntry> {
        let user_message = self.extract_user_content(user_index)?;
        let assistant_content = self.extract_assistant_content(assistant_index)?;
        let tool_call_count = self.count_tool_calls_in_range(user_index, assistant_index + 1);

        Some(crate::CompletionEntry {
            user_message: user_message.to_string(),
            assistant_content: assistant_content.to_string(),
            tool_call_count,
        })
    }

    /// Checks if there's an assistant between current user and next user (or
    /// after current user if no next user exists)
    fn has_assistant_in_range(&self, user_index: usize, next_user_index: Option<usize>) -> bool {
        match next_user_index {
            Some(next_user) => self.has_assistant_between(user_index, next_user),
            None => self.has_assistant_after(user_index),
        }
    }

    /// Finds the next user index that has an assistant after it, starting from
    /// current position in the user_indices array
    fn find_next_user_with_assistant(
        &self,
        user_indices: &[usize],
        start_position: usize,
    ) -> usize {
        let mut position = start_position + 1;
        while position < user_indices.len() {
            let user_idx = user_indices[position];
            let next_user_idx = self.find_next_user_index(user_idx);
            let has_assistant = self.has_assistant_in_range(user_idx, next_user_idx);

            if has_assistant {
                break;
            }
            position += 1;
        }
        position
    }

    /// Processes a single user-assistant pair and returns a completion entry
    fn process_user_assistant_pair(
        &self,
        user_index: usize,
        next_user_index: Option<usize>,
    ) -> Option<crate::CompletionEntry> {
        let assistant_index = self.find_last_assistant_in_range(next_user_index)?;
        self.create_entry(user_index, assistant_index)
    }

    /// Processes back-to-back user messages (multiple users without assistants
    /// between them). Returns the completion entry for the FIRST user paired
    /// with the assistant after the sequence, and the next position to
    /// continue from
    fn process_back_to_back_users(
        &self,
        user_indices: &[usize],
        start_position: usize,
    ) -> (Option<crate::CompletionEntry>, usize) {
        let end_position = self.find_next_user_with_assistant(user_indices, start_position);
        let first_user_index = user_indices[start_position];

        if end_position > start_position {
            // Find the user message after the back-to-back sequence
            // and get the last assistant before it
            let next_user_index = user_indices.get(end_position + 1).copied();
            let assistant_index = self.find_last_assistant_in_range(next_user_index);
            let entry = assistant_index
                .and_then(|assist_idx| self.create_entry(first_user_index, assist_idx));
            (entry, end_position + 1)
        } else {
            (None, start_position + 1)
        }
    }

    /// Processes all user messages and creates conversation entries by
    /// iterating through user indices and handling both normal
    /// user-assistant pairs and back-to-back user message sequences
    fn process_all_users(&self, user_indices: &[usize]) -> Vec<crate::CompletionEntry> {
        let mut entries = Vec::new();
        let mut current_position = 0;

        while current_position < user_indices.len() {
            let user_index = user_indices[current_position];
            let next_user_index = self.find_next_user_index(user_index);
            let has_assistant = self.has_assistant_in_range(user_index, next_user_index);

            if has_assistant {
                if let Some(entry) = self.process_user_assistant_pair(user_index, next_user_index) {
                    entries.push(entry);
                }
                current_position += 1;
            } else {
                let (entry, next_position) =
                    self.process_back_to_back_users(user_indices, current_position);
                if let Some(entry) = entry {
                    entries.push(entry);
                }
                current_position = next_position;
            }
        }

        entries
    }

    /// Gets a comprehensive summary of the conversation state.
    pub fn get_summary(&self) -> crate::ConversationSummary {
        let user_indices = self.find_user_message_indices();
        let entries = if user_indices.is_empty() {
            Vec::new()
        } else {
            self.process_all_users(&user_indices)
        };
        crate::ConversationSummary { entries }
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
    use crate::estimate_token_count;
    use crate::transformer::Transformer;

    // ============================================================================
    // TEST HELPERS - For building readable test contexts
    // ============================================================================

    /// Helper to create user messages with short notation
    fn user(content: &str) -> ContextMessage {
        ContextMessage::user(content, None)
    }

    /// Helper to create assistant messages with short notation
    fn assistant(content: &str) -> ContextMessage {
        ContextMessage::assistant(content, None, None)
    }

    /// Helper to create assistant messages with tool calls
    fn assistant_with_tools(content: &str, tool_count: usize) -> ContextMessage {
        let tools: Vec<ToolCallFull> = (0..tool_count)
            .map(|i| ToolCallFull {
                name: crate::ToolName::new(format!("tool_{}", i)),
                call_id: Some(crate::ToolCallId::new(format!("call_{}", i))),
                arguments: crate::ToolCallArguments::from(serde_json::json!({})),
            })
            .collect();
        ContextMessage::assistant(content, None, Some(tools))
    }

    /// Helper to create system messages
    fn system(content: &str) -> ContextMessage {
        ContextMessage::system(content)
    }

    /// Builder for creating test contexts with readable message patterns
    /// Allows patterns like: U1-U2-A1 or U1-AT2-R1-U2-A2
    struct ContextBuilder {
        messages: Vec<ContextMessage>,
    }

    impl ContextBuilder {
        fn new() -> Self {
            Self { messages: Vec::new() }
        }

        /// Add a user message
        fn u(mut self, content: &str) -> Self {
            self.messages.push(user(content));
            self
        }

        /// Add an assistant message
        fn a(mut self, content: &str) -> Self {
            self.messages.push(assistant(content));
            self
        }

        /// Add an assistant message with tool calls
        fn at(mut self, content: &str, tool_count: usize) -> Self {
            self.messages
                .push(assistant_with_tools(content, tool_count));
            self
        }

        /// Add a system message
        fn s(mut self, content: &str) -> Self {
            self.messages.push(system(content));
            self
        }

        fn build(self) -> Context {
            let mut ctx = Context::default();
            for msg in self.messages {
                ctx = ctx.add_message(msg);
            }
            ctx
        }
    }

    // ============================================================================
    // UNIT TESTS - Test individual helper functions
    // ============================================================================

    #[test]
    fn test_find_user_message_indices_empty() {
        let fixture = Context::default();
        let actual = fixture.find_user_message_indices();
        assert_eq!(actual, Vec::<usize>::new());
    }

    #[test]
    fn test_find_user_message_indices_single_user() {
        // Pattern: U1
        let fixture = ContextBuilder::new().u("U1").build();
        let actual = fixture.find_user_message_indices();
        assert_eq!(actual, vec![0]);
    }

    #[test]
    fn test_find_user_message_indices_multiple_users() {
        // Pattern: U1-A1-U2-A2-U3
        let fixture = ContextBuilder::new()
            .u("U1")
            .a("A1")
            .u("U2")
            .a("A2")
            .u("U3")
            .build();
        let actual = fixture.find_user_message_indices();
        assert_eq!(actual, vec![0, 2, 4]);
    }

    #[test]
    fn test_find_user_message_indices_with_system() {
        // Pattern: S-U1-A1-U2
        let fixture = ContextBuilder::new()
            .s("System")
            .u("U1")
            .a("A1")
            .u("U2")
            .build();
        let actual = fixture.find_user_message_indices();
        assert_eq!(actual, vec![1, 3]); // System is at 0, users at 1 and 3
    }

    #[test]
    fn test_find_last_assistant_before_exists() {
        // Pattern: U1-A1-U2
        let fixture = ContextBuilder::new().u("U1").a("A1").u("U2").build();
        let actual = fixture.find_last_assistant_before(2);
        assert_eq!(actual, Some(1));
    }

    #[test]
    fn test_find_last_assistant_before_none() {
        // Pattern: U1-U2
        let fixture = ContextBuilder::new().u("U1").u("U2").build();
        let actual = fixture.find_last_assistant_before(2);
        assert_eq!(actual, None);
    }

    #[test]
    fn test_find_last_assistant_before_multiple() {
        // Pattern: U1-A1-A2-U2 (should find last one: A2)
        let fixture = ContextBuilder::new()
            .u("U1")
            .a("A1")
            .a("A2")
            .u("U2")
            .build();
        let actual = fixture.find_last_assistant_before(3);
        assert_eq!(actual, Some(2));
    }

    #[test]
    fn test_has_assistant_between_true() {
        // Pattern: U1-A1-U2
        let fixture = ContextBuilder::new().u("U1").a("A1").u("U2").build();
        let actual = fixture.has_assistant_between(0, 2);
        assert_eq!(actual, true);
    }

    #[test]
    fn test_has_assistant_between_false() {
        // Pattern: U1-U2
        let fixture = ContextBuilder::new().u("U1").u("U2").build();
        let actual = fixture.has_assistant_between(0, 1);
        assert_eq!(actual, false);
    }

    #[test]
    fn test_has_assistant_between_invalid_range() {
        // Pattern: U1-A1
        let fixture = ContextBuilder::new().u("U1").a("A1").build();
        let actual = fixture.has_assistant_between(1, 0); // Invalid: start >= end
        assert_eq!(actual, false);
    }

    #[test]
    fn test_find_next_user_index_exists() {
        // Pattern: U1-A1-U2
        let fixture = ContextBuilder::new().u("U1").a("A1").u("U2").build();
        let actual = fixture.find_next_user_index(0);
        assert_eq!(actual, Some(2));
    }

    #[test]
    fn test_find_next_user_index_none() {
        // Pattern: U1-A1
        let fixture = ContextBuilder::new().u("U1").a("A1").build();
        let actual = fixture.find_next_user_index(0);
        assert_eq!(actual, None);
    }

    #[test]
    fn test_has_assistant_after_true() {
        // Pattern: U1-A1
        let fixture = ContextBuilder::new().u("U1").a("A1").build();
        let actual = fixture.has_assistant_after(0);
        assert_eq!(actual, true);
    }

    #[test]
    fn test_has_assistant_after_false() {
        // Pattern: U1
        let fixture = ContextBuilder::new().u("U1").build();
        let actual = fixture.has_assistant_after(0);
        assert_eq!(actual, false);
    }

    #[test]
    fn test_has_assistant_in_range_with_next_user() {
        // Pattern: U1-A1-U2
        let fixture = ContextBuilder::new().u("U1").a("A1").u("U2").build();
        let actual = fixture.has_assistant_in_range(0, Some(2));
        assert_eq!(actual, true);
    }

    #[test]
    fn test_has_assistant_in_range_without_next_user() {
        // Pattern: U1-A1
        let fixture = ContextBuilder::new().u("U1").a("A1").build();
        let actual = fixture.has_assistant_in_range(0, None);
        assert_eq!(actual, true);
    }

    #[test]
    fn test_extract_user_content_valid() {
        // Pattern: U1
        let fixture = ContextBuilder::new().u("Test content").build();
        let actual = fixture.extract_user_content(0);
        assert_eq!(actual, Some("Test content"));
    }

    #[test]
    fn test_extract_user_content_invalid_index() {
        let fixture = ContextBuilder::new().u("U1").build();
        let actual = fixture.extract_user_content(5);
        assert_eq!(actual, None);
    }

    #[test]
    fn test_extract_assistant_content_valid() {
        // Pattern: A1
        let fixture = ContextBuilder::new().a("Test response").build();
        let actual = fixture.extract_assistant_content(0);
        assert_eq!(actual, Some("Test response"));
    }

    #[test]
    fn test_create_entry_valid() {
        // Pattern: U1-A1
        let fixture = ContextBuilder::new().u("Question").a("Answer").build();
        let actual = fixture.create_entry(0, 1);

        assert!(actual.is_some());
        let entry = actual.unwrap();
        assert_eq!(entry.user_message, "Question");
        assert_eq!(entry.assistant_content, "Answer");
        assert_eq!(entry.tool_call_count, 0);
    }

    #[test]
    fn test_create_entry_with_tool_calls() {
        // Pattern: U1-AT2 (assistant with 2 tools)
        let fixture = ContextBuilder::new().u("U1").at("AT2", 2).build();
        let actual = fixture.create_entry(0, 1);

        assert!(actual.is_some());
        let entry = actual.unwrap();
        assert_eq!(entry.tool_call_count, 2);
    }

    // ============================================================================
    // INTEGRATION TESTS - Test get_summary with various message patterns
    // ============================================================================

    #[test]
    fn test_get_summary_empty_context() {
        // Pattern: (empty)
        let fixture = Context::default();
        let actual = fixture.get_summary();
        assert_eq!(actual.entries.len(), 0);
    }

    #[test]
    fn test_get_summary_user_only() {
        // Pattern: U1 (no assistant response)
        let fixture = ContextBuilder::new().u("Help me").build();
        let actual = fixture.get_summary();
        assert_eq!(actual.entries.len(), 0);
    }

    #[test]
    fn test_get_summary_u1_a1() {
        // Pattern: U1-A1
        let fixture = ContextBuilder::new().u("U1").a("A1").build();
        let actual = fixture.get_summary();

        assert_eq!(actual.entries.len(), 1);
        assert_eq!(actual.entries[0].user_message, "U1");
        assert_eq!(actual.entries[0].assistant_content, "A1");
        assert_eq!(actual.entries[0].tool_call_count, 0);
    }

    #[test]
    fn test_get_summary_u1_a1_u2_a2() {
        // Pattern: U1-A1-U2-A2
        let fixture = ContextBuilder::new()
            .u("U1")
            .a("A1")
            .u("U2")
            .a("A2")
            .build();
        let actual = fixture.get_summary();

        assert_eq!(actual.entries.len(), 2);
        assert_eq!(actual.entries[0].user_message, "U1");
        assert_eq!(actual.entries[0].assistant_content, "A1");
        assert_eq!(actual.entries[1].user_message, "U2");
        assert_eq!(actual.entries[1].assistant_content, "A2");
    }

    #[test]
    fn test_get_summary_u1_a1_a2_takes_last_assistant() {
        // Pattern: U1-A1-A2 (should take last assistant before next user)
        let fixture = ContextBuilder::new().u("U1").a("A1").a("A2").build();
        let actual = fixture.get_summary();

        assert_eq!(actual.entries.len(), 1);
        assert_eq!(actual.entries[0].user_message, "U1");
        assert_eq!(actual.entries[0].assistant_content, "A2"); // Last assistant
    }

    #[test]
    fn test_get_summary_u1_u2_a1_shows_first_user() {
        // Pattern: U1-U2-A1 (back-to-back users, should show first)
        let fixture = ContextBuilder::new().u("U1").u("U2").a("A1").build();
        let actual = fixture.get_summary();

        assert_eq!(actual.entries.len(), 1);
        assert_eq!(actual.entries[0].user_message, "U1"); // First user
        assert_eq!(actual.entries[0].assistant_content, "A1");
    }

    #[test]
    fn test_get_summary_u1_u2_u3_a1_shows_first_user() {
        // Pattern: U1-U2-U3-A1 (multiple back-to-back users)
        let fixture = ContextBuilder::new()
            .u("U1")
            .u("U2")
            .u("U3")
            .a("A1")
            .build();
        let actual = fixture.get_summary();

        assert_eq!(actual.entries.len(), 1);
        assert_eq!(actual.entries[0].user_message, "U1"); // Still first
    }

    #[test]
    fn test_get_summary_with_tool_calls() {
        // Pattern: U1-AT2 (assistant with 2 tool calls)
        let fixture = ContextBuilder::new().u("U1").at("AT2", 2).build();
        let actual = fixture.get_summary();

        assert_eq!(actual.entries.len(), 1);
        assert_eq!(actual.entries[0].tool_call_count, 2);
    }

    #[test]
    fn test_get_summary_with_system_message() {
        // Pattern: S-U1-A1-U2-A2
        let fixture = ContextBuilder::new()
            .s("System")
            .u("U1")
            .a("A1")
            .u("U2")
            .a("A2")
            .build();
        let actual = fixture.get_summary();

        assert_eq!(actual.entries.len(), 2);
        assert_eq!(actual.entries[0].user_message, "U1");
        assert_eq!(actual.entries[1].user_message, "U2");
    }

    #[test]
    fn test_get_summary_complex_pattern() {
        // Pattern: S-U1-A1-U2-U3-A2-A3-U4-A4
        let fixture = ContextBuilder::new()
            .s("System")
            .u("U1")
            .a("A1")
            .u("U2")
            .u("U3")
            .a("A2")
            .a("A3")
            .u("U4")
            .a("A4")
            .build();
        let actual = fixture.get_summary();

        assert_eq!(actual.entries.len(), 3);
        // Entry 1: U1-A1
        assert_eq!(actual.entries[0].user_message, "U1");
        assert_eq!(actual.entries[0].assistant_content, "A1");
        // Entry 2: U2-(U3)-A3 (first of back-to-back users, last assistant before U4)
        assert_eq!(actual.entries[1].user_message, "U2");
        assert_eq!(actual.entries[1].assistant_content, "A3");
        // Entry 3: U4-A4
        assert_eq!(actual.entries[2].user_message, "U4");
        assert_eq!(actual.entries[2].assistant_content, "A4");
    }

    #[test]
    fn test_get_summary_u1_a1_a2_u2_a3() {
        // Pattern: U1-A1-A2-U2-A3 (multiple assistants between users)
        let fixture = ContextBuilder::new()
            .u("U1")
            .a("A1")
            .a("A2")
            .u("U2")
            .a("A3")
            .build();
        let actual = fixture.get_summary();

        assert_eq!(actual.entries.len(), 2);
        assert_eq!(actual.entries[0].user_message, "U1");
        assert_eq!(actual.entries[0].assistant_content, "A2"); // Last before U2
        assert_eq!(actual.entries[1].user_message, "U2");
        assert_eq!(actual.entries[1].assistant_content, "A3");
    }

    #[test]
    fn test_get_summary_three_separate_exchanges() {
        // Pattern: U1-A1-U2-A2-U3-A3
        let fixture = ContextBuilder::new()
            .u("U1")
            .a("A1")
            .u("U2")
            .a("A2")
            .u("U3")
            .a("A3")
            .build();
        let actual = fixture.get_summary();

        assert_eq!(actual.entries.len(), 3);
        assert_eq!(actual.entries[0].user_message, "U1");
        assert_eq!(actual.entries[0].assistant_content, "A1");
        assert_eq!(actual.entries[1].user_message, "U2");
        assert_eq!(actual.entries[1].assistant_content, "A2");
        assert_eq!(actual.entries[2].user_message, "U3");
        assert_eq!(actual.entries[2].assistant_content, "A3");
    }

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

    #[test]
    fn test_context_is_reasoning_supported_when_enabled() {
        let fixture = Context::default()
            .reasoning(crate::agent::ReasoningConfig { enabled: Some(true), ..Default::default() });

        let actual = fixture.is_reasoning_supported();
        let expected = true;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_is_reasoning_supported_when_effort_set() {
        let fixture = Context::default().reasoning(crate::agent::ReasoningConfig {
            effort: Some(crate::agent::Effort::High),
            ..Default::default()
        });

        let actual = fixture.is_reasoning_supported();
        let expected = true;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_is_reasoning_supported_when_max_tokens_positive() {
        let fixture = Context::default().reasoning(crate::agent::ReasoningConfig {
            max_tokens: Some(1024),
            ..Default::default()
        });

        let actual = fixture.is_reasoning_supported();
        let expected = true;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_is_reasoning_not_supported_when_max_tokens_zero() {
        let fixture = Context::default()
            .reasoning(crate::agent::ReasoningConfig { max_tokens: Some(0), ..Default::default() });

        let actual = fixture.is_reasoning_supported();
        let expected = false;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_is_reasoning_not_supported_when_disabled() {
        let fixture = Context::default().reasoning(crate::agent::ReasoningConfig {
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
        let fixture = Context::default().reasoning(crate::agent::ReasoningConfig {
            enabled: Some(false),
            effort: Some(crate::agent::Effort::High), // Should be ignored when explicitly disabled
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
    fn test_find_message_with_role_before() {
        let fixture = Context::default()
            .add_message(ContextMessage::system("System message"))
            .add_message(ContextMessage::user("User question", None))
            .add_message(ContextMessage::assistant("Assistant answer", None, None));

        // Test finding system message before assistant message
        let actual = fixture.find_message_with_role_before(2, Role::System);
        assert_eq!(actual, Some("System message"));

        // Test finding user message before assistant message
        let actual = fixture.find_message_with_role_before(2, Role::User);
        assert_eq!(actual, Some("User question"));

        // Test finding assistant message before assistant message (should be None)
        let actual = fixture.find_message_with_role_before(2, Role::Assistant);
        assert_eq!(actual, None);
    }

    // ============================================================================
    // TESTS FOR ORIGINAL_CONTENT - Verify summary uses unwrapped content
    // ============================================================================

    #[test]
    fn test_get_summary_uses_original_content() {
        // Pattern: User with XML wrapping -> Assistant
        let fixture = Context::default()
            .add_message(ContextMessage::user_with_original(
                "Create a file",
                "<task>Create a file</task>",
                None,
            ))
            .add_message(ContextMessage::assistant("File created", None, None));

        let actual = fixture.get_summary();

        assert_eq!(actual.entries.len(), 1);
        // Should use original_content, NOT the formatted content with XML tags
        assert_eq!(actual.entries[0].user_message, "Create a file");
        assert_eq!(actual.entries[0].assistant_content, "File created");
    }

    #[test]
    fn test_get_summary_uses_original_content_multiple_exchanges() {
        // Pattern: Multiple exchanges with original_content
        let fixture = Context::default()
            .add_message(ContextMessage::user_with_original(
                "First task",
                "<task>First task</task>",
                None,
            ))
            .add_message(ContextMessage::assistant("First response", None, None))
            .add_message(ContextMessage::user_with_original(
                "Second task",
                "<task>Second task</task>",
                None,
            ))
            .add_message(ContextMessage::assistant("Second response", None, None));

        let actual = fixture.get_summary();

        assert_eq!(actual.entries.len(), 2);
        assert_eq!(actual.entries[0].user_message, "First task");
        assert_eq!(actual.entries[0].assistant_content, "First response");
        assert_eq!(actual.entries[1].user_message, "Second task");
        assert_eq!(actual.entries[1].assistant_content, "Second response");
    }

    #[test]
    fn test_get_summary_mixed_original_and_regular_content() {
        // Pattern: Mix of messages with and without original_content
        let fixture = Context::default()
            .add_message(ContextMessage::user_with_original(
                "Original content",
                "<task>Original content</task>",
                None,
            ))
            .add_message(ContextMessage::assistant("Response 1", None, None))
            .add_message(ContextMessage::user("Plain user message", None))
            .add_message(ContextMessage::assistant("Response 2", None, None));

        let actual = fixture.get_summary();

        assert_eq!(actual.entries.len(), 2);
        // First uses original_content
        assert_eq!(actual.entries[0].user_message, "Original content");
        // Second falls back to content (no original_content)
        assert_eq!(actual.entries[1].user_message, "Plain user message");
    }

    #[test]
    fn test_get_summary_original_content_with_feedback_tag() {
        // Pattern: User with feedback XML wrapping
        let fixture = Context::default()
            .add_message(ContextMessage::user_with_original(
                "Update the code",
                "<feedback>Update the code</feedback>",
                None,
            ))
            .add_message(ContextMessage::assistant("Code updated", None, None));

        let actual = fixture.get_summary();

        assert_eq!(actual.entries.len(), 1);
        // Should use original_content without feedback tags
        assert_eq!(actual.entries[0].user_message, "Update the code");
        assert_eq!(actual.entries[0].assistant_content, "Code updated");
    }

    #[test]
    fn test_get_summary_original_content_back_to_back_users() {
        // Pattern: Back-to-back users with original_content
        let fixture = Context::default()
            .add_message(ContextMessage::user_with_original(
                "First message",
                "<task>First message</task>",
                None,
            ))
            .add_message(ContextMessage::user_with_original(
                "Second message",
                "<feedback>Second message</feedback>",
                None,
            ))
            .add_message(ContextMessage::assistant("Combined response", None, None));

        let actual = fixture.get_summary();

        assert_eq!(actual.entries.len(), 1);
        // Should use original_content from the FIRST user message
        assert_eq!(actual.entries[0].user_message, "First message");
        assert_eq!(actual.entries[0].assistant_content, "Combined response");
    }

    #[test]
    fn test_extract_user_content_prefers_original_content() {
        // Direct test of the helper method
        let fixture = Context::default().add_message(ContextMessage::user_with_original(
            "Raw input",
            "<task>Raw input</task>",
            None,
        ));

        let actual = fixture.extract_user_content(0);

        assert_eq!(actual, Some("Raw input"));
    }

    #[test]
    fn test_extract_user_content_fallback_when_no_original() {
        // Test fallback when original_content is None
        let fixture = Context::default().add_message(ContextMessage::user("Plain message", None));

        let actual = fixture.extract_user_content(0);

        assert_eq!(actual, Some("Plain message"));
    }

    #[test]
    fn test_find_message_with_role_before_uses_original_content() {
        let fixture = Context::default()
            .add_message(ContextMessage::user_with_original(
                "Original",
                "<task>Original</task>",
                None,
            ))
            .add_message(ContextMessage::assistant("Response", None, None));

        let actual = fixture.find_message_with_role_before(1, Role::User);

        // Should return original_content, not formatted
        assert_eq!(actual, Some("Original"));
    }
}
