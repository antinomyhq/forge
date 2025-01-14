use std::path::PathBuf;

use anyhow::Result;
use inquire::Autocomplete;
use strum_macros::AsRefStr;
use tokio::fs;

use crate::console::CONSOLE;

/// Represents user input types in the chat application.
///
/// This enum encapsulates all forms of input including:
/// - System commands (starting with '/')
/// - Regular chat messages
/// - File content
#[derive(Debug, Clone)]
pub enum UserInput {
    /// End the current session and exit the application.
    /// This can be triggered with the '/end' command.
    End,
    /// Start a new conversation while preserving history.
    /// This can be triggered with the '/new' command.
    New,
    /// Reload the conversation with the original prompt.
    /// This can be triggered with the '/reload' command.
    Reload,
    /// A regular text message from the user to be processed by the chat system.
    /// Any input that doesn't start with '/' is treated as a message.
    Message(String),
}

/// Internal representation of input types including system commands.
///
/// This enum separates help commands from other user inputs to handle
/// help display without breaking the main input flow.
#[derive(Debug, AsRefStr)]
#[strum(serialize_all = "lowercase")]
enum InputKind {
    /// Display help information about available commands
    Help,
    /// Regular user input including commands and messages
    User(UserInput),
}

/// Provides command autocompletion functionality for the input prompt.
///
/// This struct maintains a list of available commands and implements
/// the Autocomplete trait to provide suggestions as the user types.
#[derive(Clone)]
struct CommandCompleter {
    commands: Vec<String>,
}

impl CommandCompleter {
    /// Creates a new command completer with the list of available commands
    fn new() -> Self {
        Self { commands: InputKind::available_commands() }
    }
}

impl Autocomplete for CommandCompleter {
    /// Returns a list of command suggestions that match the current input.
    /// Only provides suggestions for inputs starting with '/'.
    fn get_suggestions(
        &mut self,
        input: &str,
    ) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
        if input.starts_with('/') {
            Ok(self
                .commands
                .iter()
                .filter(|cmd| cmd.starts_with(input))
                .cloned()
                .collect())
        } else {
            Ok(vec![])
        }
    }

    /// Returns the best matching command completion for the current input.
    /// Only provides completions for inputs starting with '/'.
    fn get_completion(
        &mut self,
        input: &str,
        highlighted_suggestion: Option<String>,
    ) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(highlighted_suggestion.or_else(|| {
            if input.starts_with('/') {
                self.commands
                    .iter()
                    .find(|cmd| cmd.starts_with(input))
                    .cloned()
            } else {
                None
            }
        }))
    }
}

impl InputKind {
    /// Returns a list of all available command strings.
    ///
    /// These commands are used for:
    /// - Command validation
    /// - Autocompletion
    /// - Help display
    fn available_commands() -> Vec<String> {
        vec![
            "/end".to_string(),
            "/new".to_string(),
            "/reload".to_string(),
            "/help".to_string(),
        ]
    }

    /// Parses a string input into an InputKind.
    ///
    /// This function:
    /// - Trims whitespace from the input
    /// - Recognizes and validates commands (starting with '/')
    /// - Converts regular text into messages
    ///
    /// # Returns
    /// - `Ok(InputKind)` - Successfully parsed input
    /// - `Err` - Input was an invalid command
    fn parse(input: &str) -> Result<Self> {
        let trimmed = input.trim();
        match trimmed {
            "/end" => Ok(InputKind::User(UserInput::End)),
            "/new" => Ok(InputKind::User(UserInput::New)),
            "/reload" => Ok(InputKind::User(UserInput::Reload)),
            "/help" => Ok(InputKind::Help),
            cmd if cmd.starts_with('/') => {
                let available_commands = Self::available_commands();
                Err(anyhow::anyhow!(
                    "Unknown command: '{}'. Available commands: {}",
                    cmd,
                    available_commands.join(", ")
                ))
            }
            text => Ok(InputKind::User(UserInput::Message(text.to_string()))),
        }
    }

    /// Returns a formatted help text describing all available commands and
    /// usage instructions.
    ///
    /// The help text includes:
    /// - List of available commands with descriptions
    /// - General usage instructions
    /// - Quick guide for common operations
    fn get_help_text() -> String {
        "Available Commands:
/new    - Start a new conversation with a clean history. You will need to provide
          the initial prompt or task to begin the conversation.

/reload - Reset and restart the current conversation using the original prompt
          or file that started this session. This preserves the initial context
          but clears all subsequent conversation history.

/end    - End the current conversation and exit the application.

Other Usage:
- Type your message directly to chat with the AI
- Commands are case-sensitive and must start with '/'
- Use tab completion to quickly enter commands
- Arrow keys can be used to navigate command history

Quick Guide:
- Use /new when you want to start a completely fresh conversation
- Use /reload when you want to retry the current task from the beginning
- Use /end when you're finished with all conversations"
            .to_string()
    }
}

impl UserInput {
    /// Read content from a file and return it as a UserInput.
    ///
    /// # Arguments
    /// * `path` - The path to the file to read
    ///
    /// # Returns
    /// * `Ok(UserInput::File)` - Successfully read file content
    /// * `Err` - Failed to read file
    pub async fn from_file<P: Into<PathBuf>>(path: P) -> Result<Self> {
        let path = path.into();
        let content = fs::read_to_string(&path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read file {}: {}", path.display(), e))?
            .trim()
            .to_string();

        CONSOLE.writeln(content.clone())?;
        Ok(UserInput::Message(content))
    }

    /// Prompts for user input with help text.
    ///
    /// # Arguments
    /// * `initial_prompt` - Whether this is an initial conversation prompt
    /// * `help_text` - Optional help text to display with the prompt
    ///
    /// # Returns
    /// - `Ok(UserInput)` - Successfully processed input
    /// - `Err` - An error occurred during input processing
    pub fn prompt(help_text: Option<&str>, initial_text: Option<&str>) -> Result<Self> {
        loop {
            CONSOLE.writeln("")?;
            let help = help_text.map(|a| a.to_string()).unwrap_or(format!(
                "How can I help? Available commands: {}",
                InputKind::available_commands().join(", ")
            ));

            let mut text = inquire::Text::new("")
                .with_help_message(&help)
                .with_autocomplete(CommandCompleter::new());

            if let Some(initial_text) = initial_text {
                text = text.with_initial_value(initial_text);
            }

            let text = text.prompt()?;

            match InputKind::parse(&text) {
                Ok(input_kind) => match input_kind {
                    InputKind::Help => {
                        CONSOLE.writeln(format!("\n{}\n", InputKind::get_help_text()))?;
                        continue;
                    }
                    InputKind::User(input) => return Ok(input),
                },
                Err(e) => {
                    eprintln!("Error: {}", e);
                }
            }
        }
    }
}
