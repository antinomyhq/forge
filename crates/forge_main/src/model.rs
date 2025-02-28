use std::collections::BTreeMap;

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

        let mut models_by_provider: BTreeMap<String, Vec<&Model>> = BTreeMap::new();
        for model in models {
            let provider = model
                .id
                .as_str()
                .split('/')
                .next()
                .unwrap_or("unknown")
                .to_string();
            models_by_provider.entry(provider).or_default().push(model);
        }

        for (provider, provider_models) in models_by_provider.iter() {
            info = info.add_title(provider.to_string());
            for model in provider_models {
                if let Some(context_length) = model.context_length {
                    info = info.add_item(
                        &model.name,
                        format!("{} ({})", model.id, humanize_context_length(context_length)),
                    );
                } else {
                    info = info.add_item(&model.name, format!("{}", model.id));
                }
            }
        }

        info
    }
}

use std::path::PathBuf;

use async_trait::async_trait;

/// Represents user input types in the chat application.
///
/// This enum encapsulates all forms of input including:
/// - System commands (starting with '/')
/// - Regular chat messages
/// - File content
/// - Custom dispatch commands (starting with '/dispatch-')
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
    /// Dumps the current conversation into a json file
    Dump,
    /// Custom dispatch command with event name and value.
    /// This can be triggered with '/dispatch-{event_name} {value}'.
    /// Event names can use alphanumeric characters, underscores, and hyphens.
    Dispatch {
        /// The name of the event to dispatch
        name: String,
        /// The value to pass with the event (can be empty)
        value: String,
    },
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
            "/dump".to_string(),
            "/dispatch-".to_string(), // Base dispatch command for autocompletion
        ]
    }

    /// Validates if a string is a valid event name.
    /// Event names can only contain alphanumeric characters, underscores, and hyphens.
    fn is_valid_event_name(name: &str) -> bool {
        !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    }

    /// Parses a string input into an Input.
    ///
    /// This function:
    /// - Trims whitespace from the input
    /// - Recognizes and validates commands (starting with '/')
    /// - Handles dispatch commands (starting with '/dispatch-')
    /// - Converts regular text into messages
    ///
    /// # Returns
    /// - `Ok(Input)` - Successfully parsed input
    /// - `Err` - Input was an invalid command
    pub fn parse(input: &str) -> Self {
        let trimmed = input.trim();

        // First check for standard commands
        match trimmed {
            "/new" => return Command::New,
            "/info" => return Command::Info,
            "/exit" => return Command::Exit,
            "/models" => return Command::Models,
            "/dump" => return Command::Dump,
            _ => {}
        }

        // Check for dispatch commands
        if let Some(dispatch_part) = trimmed.strip_prefix("/dispatch-") {
            // Split at first space to separate event name and value
            let (event_name, event_value) = match dispatch_part.find(' ') {
                Some(space_idx) => {
                    let (name, value) = dispatch_part.split_at(space_idx);
                    (name, value.trim())
                }
                None => (dispatch_part, ""), // No space found, entire part is event name
            };

            // Validate event name
            if Self::is_valid_event_name(event_name) {
                return Command::Dispatch {
                    name: event_name.to_string(),
                    value: event_value.to_string(),
                };
            }
        }

        // If not a command, treat as regular message
        Command::Message(trimmed.to_string())
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
        // Test valid dispatch commands
        assert_eq!(
            Command::parse("/dispatch-gh-issue Create new issue"),
            Command::Dispatch {
                name: "gh-issue".to_string(),
                value: "Create new issue".to_string()
            }
        );

        assert_eq!(
            Command::parse("/dispatch-notify"),
            Command::Dispatch {
                name: "notify".to_string(),
                value: "".to_string()
            }
        );

        assert_eq!(
            Command::parse("/dispatch-log-error Error with spaces"),
            Command::Dispatch {
                name: "log-error".to_string(),
                value: "Error with spaces".to_string()
            }
        );

        // Test invalid dispatch commands
        assert_eq!(
            Command::parse("/dispatch-"),
            Command::Message("/dispatch-".to_string())
        );

        // Both "/dispatch-" and "/dispatch- " should be treated the same after trimming
        assert_eq!(
            Command::parse("/dispatch-"),
            Command::parse("/dispatch- ")
        );
    }

    #[test]
    fn test_event_name_validation() {
        // Valid event names
        assert!(Command::is_valid_event_name("test"));
        assert!(Command::is_valid_event_name("test-name"));
        assert!(Command::is_valid_event_name("test_name"));
        assert!(Command::is_valid_event_name("test123"));
        assert!(Command::is_valid_event_name("123test"));

        // Invalid event names
        assert!(!Command::is_valid_event_name(""));
        assert!(!Command::is_valid_event_name(" "));
        assert!(!Command::is_valid_event_name("test name"));
        assert!(!Command::is_valid_event_name("test.name"));
        assert!(!Command::is_valid_event_name("test/name"));
    }
}
