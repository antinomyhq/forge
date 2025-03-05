use std::collections::BTreeMap;
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
    /// Dumps the current conversation into a json file
    Dump,
    /// Retries the last user message in the conversation
    Retry,
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
            "/retry".to_string(),
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

        match trimmed {
            "/new" => Command::New,
            "/info" => Command::Info,
            "/exit" => Command::Exit,
            "/models" => Command::Models,
            "/dump" => Command::Dump,
            "/retry" => Command::Retry,
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
