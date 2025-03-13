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
    /// Dispatches a custom event.
    /// This can be triggered with the '/dispatch-event_name value' command format.
    Dispatch(String, String),
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
            "/dispatch-".to_string(),  // Base prefix for dispatch commands
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

        // Check for standard commands
        match trimmed {
            "/new" => Command::New,
            "/info" => Command::Info,
            "/exit" => Command::Exit,
            "/models" => Command::Models,
            "/dump" => Command::Dump,
            "/act" => Command::Act,
            "/plan" => Command::Plan,
            "/help" => Command::Help,
            // If it starts with "/dispatch-", parse as a dispatch command
            text if text.starts_with("/dispatch-") => {
                // Strip the "/dispatch-" prefix
                let text = &text["/dispatch-".len()..];
                
                // Find the first space to separate event name and value
                if let Some(space_idx) = text.find(char::is_whitespace) {
                    // Split into event name and value
                    let event_name = &text[..space_idx];
                    let event_value = &text[space_idx + 1..];
                    Command::Dispatch(event_name.to_string(), event_value.to_string())
                } else {
                    // No space found, so the entire text is the event name and the value is empty
                    Command::Dispatch(text.to_string(), String::new())
                }
            },
            // Default case - treat as a regular message
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
    fn test_dispatch_command_parsing() {
        // Test basic dispatch command with value
        let cmd = Command::parse("/dispatch-test value");
        match cmd {
            Command::Dispatch(name, value) => {
                assert_eq!(name, "test");
                assert_eq!(value, "value");
            }
            _ => panic!("Expected Dispatch command"),
        }
        
        // Test dispatch command without value (empty value)
        let cmd = Command::parse("/dispatch-test");
        match cmd {
            Command::Dispatch(name, value) => {
                assert_eq!(name, "test");
                assert_eq!(value, "");
            }
            _ => panic!("Expected Dispatch command"),
        }
        
        // Test dispatch command with value containing spaces
        let cmd = Command::parse("/dispatch-github create issue for updating dependencies");
        match cmd {
            Command::Dispatch(name, value) => {
                assert_eq!(name, "github");
                assert_eq!(value, "create issue for updating dependencies");
            }
            _ => panic!("Expected Dispatch command"),
        }
    }

    #[test]
    fn test_standard_commands() {
        // Ensure our modifications don't break standard commands
        assert!(matches!(Command::parse("/new"), Command::New));
        assert!(matches!(Command::parse("/exit"), Command::Exit));
        assert!(matches!(Command::parse("/info"), Command::Info));
        assert!(matches!(Command::parse("/models"), Command::Models));
        assert!(matches!(Command::parse("/act"), Command::Act));
        assert!(matches!(Command::parse("/plan"), Command::Plan));
        assert!(matches!(Command::parse("/help"), Command::Help));
        assert!(matches!(Command::parse("/dump"), Command::Dump));
        
        // Test a regular message
        match Command::parse("Hello, world") {
            Command::Message(msg) => assert_eq!(msg, "Hello, world"),
            _ => panic!("Expected Message command"),
        }
    }
}