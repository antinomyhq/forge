use std::path::PathBuf;

use async_trait::async_trait;
use forge_api::Model;

use crate::info::Info;

fn humanize_context_length(length: u64) -> String {
    if length >= 1_000_000 {
        format!("{:.1}M context", length as f64 / 1_000_000.0)
    } else if length >= 1_000 {
        format!("{:.1}K context", length as f64 / 1_000.0)
    } else {
        format!("{} context", length)
    }
}

impl From<&[Model]> for Info {
    fn from(models: &[Model]) -> Self {
        let mut info = Info::new();

        for model in models.iter() {
            if let Some(context_length) = model.context_length {
                info = info.add_key_value(&model.id, humanize_context_length(context_length));
            } else {
                info = info.add_key(&model.id);
            }
        }

        info
    }
}

/// Represents user input types in the chat application.
///
/// This enum encapsulates all forms of input including:
/// - System commands (starting with '/')
/// - Regular chat messages
/// - File content
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Custom command dispatch that triggers event handling with a format of `/dispatch-event_name value`
    /// The event_name must follow specific formatting rules (alphanumeric, plus hyphens and underscores)
    Dispatch(String, String),
    /// Start a new conversation while preserving history.
    /// This can be triggered with the '/new' command.
    New,
    /// A regular text message from the user to be processed by the chat system.
    /// Any input that doesn't start with '/' is treated as a message.
    Message(String),
    /// Display system environment information.
    /// This can be triggered with the '/info' command.
    Info,
    /// Exit the application without any further action.
    Exit,
    /// Lists the models available for use.
    Models,
    /// Switch to "act" mode.
    /// This can be triggered with the '/act' command.
    Act,
    /// Switch to "plan" mode.
    /// This can be triggered with the '/plan' command.
    Plan,
    /// Switch to "help" mode.
    /// This can be triggered with the '/help' command.
    Help,
    /// Dumps the current conversation into a json file
    Dump,
}

impl Command {
    /// Returns a list of all available command strings.
    ///
    /// These commands are used for:
    /// - Command validation
    /// - Autocompletion
    /// - Help display
    pub fn available_commands() -> Vec<String> {
        vec![
            "/new".to_string(),
            "/info".to_string(),
            "/exit".to_string(),
            "/models".to_string(),
            "/act".to_string(),
            "/plan".to_string(),
            "/help".to_string(),
            "/dump".to_string(),
            "/dispatch-event_name".to_string(),
        ]
    }

    /// Parses a string input into an Input.
    ///
    /// This function:
    /// - Trims whitespace from the input
    /// - Recognizes and validates commands (starting with '/')
    /// - Converts regular text into messages
    ///
    /// # Returns
    /// - `Ok(Input)` - Successfully parsed input
    /// - `Err` - Input was an invalid command
    pub fn parse(input: &str) -> Self {
        let trimmed = input.trim();

        // Check if this is a dispatch command
        if trimmed.starts_with("/dispatch-") {
            // Get everything after "/dispatch-" until a space or end of string
            let (event_name, value) = match trimmed[10..].find(' ') {
                Some(space_index) => {
                    let event_name = &trimmed[10..10 + space_index];
                    let value = &trimmed[10 + space_index + 1..];
                    (event_name.to_string(), value.to_string())
                }
                None => {
                    // No space found, so everything after "/dispatch-" is the event name
                    // and value is empty
                    (trimmed[10..].to_string(), "".to_string())
                }
            };
            
            // Validate event name - only allow alphanumeric, underscores, and hyphens
            if event_name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
                return Command::Dispatch(event_name, value);
            }
            // If event name is invalid, treat as a regular message
        }
        
        match trimmed {
            "/new" => Command::New,
            "/info" => Command::Info,
            "/exit" => Command::Exit,
            "/models" => Command::Models,
            "/dump" => Command::Dump,
            "/act" => Command::Act,
            "/plan" => Command::Plan,
            "/help" => Command::Help,
            text => Command::Message(text.to_string()),
        }
    }
}

/// A trait for handling user input in the application.
///
/// This trait defines the core functionality needed for processing
/// user input, whether it comes from a command line interface,
/// GUI, or file system.
#[async_trait]
pub trait UserInput {
    type PromptInput;
    /// Read content from a file and convert it to the input type.
    ///
    /// # Arguments
    /// * `path` - The path to the file to read
    ///
    /// # Returns
    /// * `Ok(Input)` - Successfully read and parsed file content
    /// * `Err` - Failed to read or parse file
    async fn upload<P: Into<PathBuf> + Send>(&self, path: P) -> anyhow::Result<Command>;

    /// Prompts for user input with optional help text and initial value.
    ///
    /// # Arguments
    /// * `help_text` - Optional help text to display with the prompt
    /// * `initial_text` - Optional initial text to populate the input with
    ///
    /// # Returns
    /// * `Ok(Input)` - Successfully processed input
    /// * `Err` - An error occurred during input processing
    async fn prompt(&self, input: Option<Self::PromptInput>) -> anyhow::Result<Command>;
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dispatch_command() {
        // Test valid dispatch command with value
        let input = "/dispatch-test_event This is a test value";
        match Command::parse(input) {
            Command::Dispatch(event_name, value) => {
                assert_eq!(event_name, "test_event");
                assert_eq!(value, "This is a test value");
            }
            _ => panic!("Failed to parse valid dispatch command"),
        }
        
        // Test valid dispatch command with no value
        let input = "/dispatch-empty_event";
        match Command::parse(input) {
            Command::Dispatch(event_name, value) => {
                assert_eq!(event_name, "empty_event");
                assert_eq!(value, "");
            }
            _ => panic!("Failed to parse valid dispatch command without value"),
        }
        
        // Test dispatch command with hyphens and underscores
        let input = "/dispatch-custom-event_name Some value";
        match Command::parse(input) {
            Command::Dispatch(event_name, value) => {
                assert_eq!(event_name, "custom-event_name");
                assert_eq!(value, "Some value");
            }
            _ => panic!("Failed to parse valid dispatch command with hyphens and underscores"),
        }
        
        // Test invalid dispatch command (contains invalid characters)
        let input = "/dispatch-invalid!event Value";
        match Command::parse(input) {
            Command::Message(message) => {
                assert_eq!(message, input);
            }
            _ => panic!("Invalid dispatch command should be treated as a message"),
        }
    }
}