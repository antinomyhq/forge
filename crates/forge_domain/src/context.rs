use std::collections::HashSet;

use derive_more::derive::{Display, From};
use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::{ToolCallFull, ToolResult};
use crate::{ToolChoice, ToolDefinition};

#[derive(
    Debug, schemars::JsonSchema, serde::Deserialize, serde::Serialize, Clone, PartialEq, Eq, Hash,
)]
pub struct Attachment {
    pub content: String,
    pub path: String,
    pub content_type: ContentType,
}

impl Attachment {
    /// Parses a string and extracts all file paths prefixed with "@".
    /// File paths can contain spaces and are considered to extend until the
    /// next whitespace. When a file path contains spaces, the entire path
    /// should be wrapped in quotes.
    pub fn parse_all<T: ToString>(v: T) -> HashSet<String> {
        let v = v.to_string();
        let mut paths = HashSet::new();
        let mut i = 0;

        while i < v.len() {
            let remaining = &v[i..];

            if let Some(pos) = remaining.find('@') {
                i += pos + 1; // Move past the '@'

                if i >= v.len() {
                    break;
                }

                let path_start = i;
                let path_end;

                // Check if the path is quoted (for paths with spaces)
                if i < v.len() && &v[i..i + 1] == "\"" {
                    i += 1; // Move past the opening quote
                    let path_start_after_quote = i;

                    // Find the closing quote
                    if let Some(end_quote) = v[i..].find('\"') {
                        path_end = i + end_quote;
                        let file_path = v[path_start_after_quote..path_end].to_string();

                        // Add the file path to the set
                        paths.insert(file_path);

                        i = path_end + 1; // Move past the closing quote
                    } else {
                        // If no closing quote, consider the rest of the string as path
                        path_end = v.len();
                        let file_path = v[path_start_after_quote..path_end].to_string();

                        // Add the file path to the set
                        paths.insert(file_path);

                        i = path_end;
                    }
                    continue; // Skip the common path handling code since we've
                              // already added the attachment
                } else {
                    // For unquoted paths, the path extends until the next whitespace
                    if let Some(end_pos) = v[i..].find(char::is_whitespace) {
                        path_end = i + end_pos;
                        i = path_end; // Move to the whitespace
                    } else {
                        // If no whitespace, consider the rest of the string as path
                        path_end = v.len();
                        i = path_end;
                    }
                }

                let file_path = if path_start < path_end {
                    v[path_start..path_end].to_string()
                } else {
                    continue;
                };

                // Add the file path to the set
                paths.insert(file_path);
            } else {
                break;
            }
        }

        paths
    }
}

#[derive(
    Debug, schemars::JsonSchema, serde::Deserialize, serde::Serialize, Clone, PartialEq, Eq, Hash,
)]
pub enum ContentType {
    Image,
    Text,
}

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
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_attachment_parse_all_empty() {
        let text = String::from("No attachments here");
        let attachments = Attachment::parse_all(text);
        assert!(attachments.is_empty());
    }

    #[test]
    fn test_attachment_parse_all_simple() {
        let text = String::from("Check this file @/path/to/file.txt");
        let paths = Attachment::parse_all(text);
        assert_eq!(paths.len(), 1);

        let path_found = paths.iter().next().unwrap();
        assert_eq!(path_found, "/path/to/file.txt");
    }

    #[test]
    fn test_attachment_parse_all_with_spaces() {
        let text = String::from("Check this file @\"/path/with spaces/file.txt\"");
        let paths = Attachment::parse_all(text);
        assert_eq!(paths.len(), 1);

        let path_found = paths.iter().next().unwrap();
        assert_eq!(path_found, "/path/with spaces/file.txt");
    }

    #[test]
    fn test_attachment_parse_all_multiple() {
        let text = String::from(
            "Check @/file1.txt and also @\"/path/with spaces/file2.txt\" and @/file3.txt",
        );
        let paths = Attachment::parse_all(text);
        assert_eq!(paths.len(), 3);

        assert!(paths.contains("/file1.txt"));
        assert!(paths.contains("/path/with spaces/file2.txt"));
        assert!(paths.contains("/file3.txt"));
    }

    #[test]
    fn test_attachment_parse_all_at_end() {
        let text = String::from("Check this file @");
        let paths = Attachment::parse_all(text);
        assert_eq!(paths.len(), 0);
    }

    #[test]
    fn test_attachment_parse_all_unclosed_quote() {
        let text = String::from("Check this file @\"/path/with spaces/unclosed");
        let paths = Attachment::parse_all(text);
        assert_eq!(paths.len(), 1);

        let path_found = paths.iter().next().unwrap();
        assert_eq!(path_found, "/path/with spaces/unclosed");
    }

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
}
