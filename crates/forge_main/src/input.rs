use anyhow::Result;
use inquire::Autocomplete;
use strum_macros::AsRefStr;

use crate::console::CONSOLE;

/// Represents the different types of user inputs that can be processed by the
/// system.
#[derive(Debug)]
pub enum UserInput {
    /// End the current session and exit the application
    End,
    /// Start a new conversation while preserving history
    New,
    /// Reload the conversation by clearing the conversation ID and starting
    /// fresh
    Reload,
    /// A regular text message from the user to be processed
    Message(String),
}

/// Internal representation of input types including system commands
#[derive(Debug, AsRefStr)]
#[strum(serialize_all = "lowercase")]
enum InputKind {
    /// Display help information about available commands
    Help,
    /// Regular user input including commands and messages
    User(UserInput),
}

/// Provides command autocompletion functionality for the input prompt
#[derive(Clone)]
struct CommandCompleter {
    commands: Vec<String>,
}

impl CommandCompleter {
    fn new() -> Self {
        Self { commands: InputKind::available_commands() }
    }
}

impl Autocomplete for CommandCompleter {
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
    /// Returns a list of all available command strings
    fn available_commands() -> Vec<String> {
        vec![
            "/end".to_string(),
            "/new".to_string(),
            "/reload".to_string(),
            "/help".to_string(),
        ]
    }

    /// Parses a string input into an InputKind, handling both commands and
    /// regular messages
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
    /// usage instructions
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
    /// Prompts for user input, handling commands and messages appropriately.
    /// Continues prompting if help is requested.
    pub fn prompt() -> Result<Self> {
        loop {
            CONSOLE.writeln("")?;
            let text = inquire::Text::new("")
                .with_help_message(&format!(
                    "Available commands: {}",
                    InputKind::available_commands().join(", ")
                ))
                .with_autocomplete(CommandCompleter::new())
                .prompt()?;

            match InputKind::parse(&text)? {
                InputKind::Help => {
                    CONSOLE.writeln(format!("\n{}\n", InputKind::get_help_text()))?;
                    continue;
                }
                InputKind::User(input) => return Ok(input),
            }
        }
    }

    /// Prompts for the initial input when starting a conversation.
    /// Only accepts messages and help commands, ignoring other commands.
    pub fn prompt_initial() -> Result<String> {
        loop {
            let text = inquire::Text::new("")
                .with_help_message(&format!(
                    "How can I help? Available commands: {}",
                    InputKind::available_commands().join(", ")
                ))
                .with_autocomplete(CommandCompleter::new())
                .prompt()?;

            match InputKind::parse(&text)? {
                InputKind::Help => {
                    CONSOLE.writeln(format!("\n{}\n", InputKind::get_help_text()))?;
                    continue;
                }
                InputKind::User(UserInput::Message(msg)) => return Ok(msg),
                InputKind::User(_) => continue, // Ignore other commands in initial prompt
            }
        }
    }
}
