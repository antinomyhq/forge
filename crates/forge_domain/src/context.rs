use derive_more::derive::{Display, From};
use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::{ToolCallFull, ToolResult};
use crate::temperature::Temperature;
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
}

impl Context {
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

    pub fn extend_tools(mut self, tools: Vec<impl Into<ToolDefinition>>) -> Self {
        self.tools.extend(tools.into_iter().map(Into::into));
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
            }
        }

        format!("<chat_history>{lines}</chat_history>")
    }

    /// Estimates the token count for this context using a simple approximation
    ///
    /// This method uses a basic character-to-token ratio to approximate tokens.
    /// For more accurate token counts, a proper model-specific tokenizer should
    /// be used.
    pub fn estimate_token_count(&self) -> u64 {
        // Call the standalone function from agent.rs with the text representation of
        // this context
        crate::estimate_token_count(&self.to_text())
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
                    crate::ToolOutputValue::Empty => {}
                }
            }
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

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
        let token_count = context.estimate_token_count();

        // Validate the token count is reasonable
        // The exact value will depend on the implementation of estimate_token_count
        assert!(token_count > 0, "Token count should be greater than 0");
    }
}
