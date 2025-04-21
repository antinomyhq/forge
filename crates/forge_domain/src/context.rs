use derive_more::derive::{Display, From};
use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::{ToolCallFull, ToolResult};
use crate::temperature::Temperature;
use crate::{ToolCallRecord, ToolChoice, ToolDefinition};

/// Represents a message being sent to the LLM provider
/// NOTE: ToolResults message are part of the larger Request object and not part
/// of the message.
#[derive(Clone, Debug, Deserialize, From, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextMessage {
    ContentMessage(ContentMessage),
    ToolMessage(ToolResult),
    Image(String),
}

impl ContextMessage {
    pub fn user(content: impl ToString) -> Self {
        ContentMessage {
            role: Role::User,
            content: content.to_string(),
            tool_calls: None,
        }
        .into()
    }

    pub fn system(content: impl ToString) -> Self {
        ContentMessage {
            role: Role::System,
            content: content.to_string(),
            tool_calls: None,
        }
        .into()
    }

    pub fn assistant(content: impl ToString, tool_calls: Option<Vec<ToolCallFull>>) -> Self {
        let tool_calls =
            tool_calls.and_then(|calls| if calls.is_empty() { None } else { Some(calls) });
        ContentMessage {
            role: Role::Assistant,
            content: content.to_string(),
            tool_calls,
        }
        .into()
    }

    pub fn tool_result(result: ToolResult) -> Self {
        Self::ToolMessage(result)
    }

    pub fn has_role(&self, role: Role) -> bool {
        match self {
            ContextMessage::ContentMessage(message) => message.role == role,
            ContextMessage::ToolMessage(_) => false,
            ContextMessage::Image(_) => Role::User == role,
        }
    }

    pub fn has_tool_call(&self) -> bool {
        match self {
            ContextMessage::ContentMessage(message) => message.tool_calls.is_some(),
            ContextMessage::ToolMessage(_) => false,
            ContextMessage::Image(_) => false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Setters)]
#[setters(strip_option, into)]
#[serde(rename_all = "snake_case")]
pub struct ContentMessage {
    pub role: Role,
    pub content: String,
    pub tool_calls: Option<Vec<ToolCallFull>>,
}

impl ContentMessage {
    pub fn assistant(content: impl ToString) -> Self {
        Self {
            role: Role::Assistant,
            content: content.to_string(),
            tool_calls: None,
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
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Setters, Default)]
#[setters(into, strip_option)]
pub struct Context {
    pub messages: Vec<ContextMessage>,
    pub tools: Vec<ToolDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<Temperature>,
}

impl Context {
    pub fn add_url(mut self, url: &str) -> Self {
        self.messages.push(ContextMessage::Image(url.to_string()));
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
            if let Some(ContextMessage::ContentMessage(content_message)) = self.messages.get_mut(0)
            {
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
                ContextMessage::ContentMessage(message) => {
                    lines.push_str(&format!("<message role=\"{}\">", message.role));
                    lines.push_str(&format!("<content>{}</content>", message.content));
                    if let Some(tool_calls) = &message.tool_calls {
                        for call in tool_calls {
                            lines.push_str(&format!(
                                "<tool_call name=\"{}\"><![CDATA[{}]]></tool_call>",
                                call.name.as_str(),
                                serde_json::to_string(&call.arguments).unwrap()
                            ));
                        }
                    }

                    lines.push_str("</message>");
                }
                ContextMessage::ToolMessage(result) => {
                    lines.push_str("<message role=\"tool\">");

                    lines.push_str(&format!(
                        "<tool_result name=\"{}\"><![CDATA[{}]]></tool_result>",
                        result.name.as_str(),
                        serde_json::to_string(&result.content).unwrap()
                    ));
                    lines.push_str("</message>");
                }
                ContextMessage::Image(url) => {
                    lines.push_str(format!("<file_attachment path=\"{}\">", url).as_str());
                }
            }
        }

        format!("<chat_history>{}</chat_history>", lines)
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
        tool_records: Vec<ToolCallRecord>,
        tool_supported: bool,
    ) -> Self {
        if tool_supported {
            self.add_message(ContextMessage::assistant(
                content,
                Some(
                    tool_records
                        .iter()
                        .map(|record| record.tool_call.clone())
                        .collect::<Vec<_>>(),
                ),
            ))
            .add_tool_results(
                tool_records
                    .iter()
                    .map(|record| record.tool_result.clone())
                    .collect::<Vec<_>>(),
            )
        } else {
            self = self.add_message(ContextMessage::assistant(content, None));
            if tool_records.is_empty() {
                return self;
            }
            let content = tool_records.iter().fold(String::new(), |mut acc, result| {
                if !acc.is_empty() {
                    acc.push_str("\n\n");
                }
                acc.push_str(result.to_string().as_str());
                acc
            });

            self.add_message(ContextMessage::user(content))
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
            ContextMessage::system("Updated system message")
        );
    }

    #[test]
    fn test_set_system_message() {
        let request = Context::default().set_first_system_message("A system message");

        assert_eq!(
            request.messages[0],
            ContextMessage::system("A system message")
        );
    }

    #[test]
    fn test_insert_system_message() {
        let request = Context::default()
            .add_message(ContextMessage::user("Do something"))
            .set_first_system_message("A system message");

        assert_eq!(
            request.messages[0],
            ContextMessage::system("A system message")
        );
    }

    #[test]
    fn test_estimate_token_count() {
        // Create a context with some messages
        let context = Context::default()
            .add_message(ContextMessage::system("System message"))
            .add_message(ContextMessage::user("User message"))
            .add_message(ContextMessage::assistant("Assistant message", None));

        // Get the token count
        let token_count = context.estimate_token_count();

        // Validate the token count is reasonable
        // The exact value will depend on the implementation of estimate_token_count
        assert!(token_count > 0, "Token count should be greater than 0");
    }
}
