use std::collections::BTreeMap;

use forge_domain::Model;

use crate::error::{Error, Result};
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
                info = info.add_item(
                    &model.name,
                    format!(
                        "{} ({})",
                        model.id,
                        humanize_context_length(model.context_length)
                    ),
                );
            }
        }

        info
    }
}

use std::path::PathBuf;
use std::str::FromStr;

use async_trait::async_trait;
use strum_macros::{Display, EnumString};

#[derive(Debug, Clone, PartialEq, Eq, Hash, EnumString, Display)]
pub enum ConfigKey {
    #[strum(serialize = "primary-model")]
    PrimaryModel,
    #[strum(serialize = "secondary-model")]
    SecondaryModel,
    #[strum(serialize = "tool-timeout")]
    ToolTimeout,
}

impl ConfigKey {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConfigKey::PrimaryModel => "primary-model",
            ConfigKey::SecondaryModel => "secondary-model",
            ConfigKey::ToolTimeout => "tool-timeout",
        }
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
    /// Display system environment information.
    /// This can be triggered with the '/info' command.
    Info,
    /// Exit the application without any further action.
    Exit,
    /// Lists the models available for use.
    Models,
    /// Config command, can be used to get or set or display configuration
    /// values.
    Config(ConfigCommand),
    /// Retries the last request irrespective of it's failed or not.
    Retry,
}

/// Represents different configuration operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigCommand {
    /// List all available configuration options
    List,
    /// Get the value of a specific configuration key
    Get(ConfigKey),
    /// Set a configuration key to a specific value
    Set(ConfigKey, String),
}

impl ConfigCommand {
    /// Parse a config command from string arguments
    ///
    /// # Arguments
    /// * `args` - Command arguments (without "config" command itself)
    ///
    /// # Returns
    /// * `Ok(ConfigCommand)` - Successfully parsed command
    /// * `Err` - Parse error with usage information
    fn parse(args: &[&str]) -> Result<ConfigCommand> {
        // No arguments = list command
        if args.is_empty() {
            return Ok(ConfigCommand::List);
        }

        // Get command type and ensure it's valid
        match args.first().copied() {
            Some("get") => {
                let key_str = args
                    .get(1)
                    .ok_or_else(|| Error::MissingParameter("key".into()))?;
                let key = ConfigKey::from_str(key_str)
                    .map_err(|_| Error::UnsupportedParameter(key_str.to_string()))?;
                Ok(ConfigCommand::Get(key))
            }
            Some("set") => {
                let key_str = args
                    .get(1)
                    .ok_or_else(|| Error::MissingParameter("key".into()))?;
                let key = ConfigKey::from_str(key_str)
                    .map_err(|_| Error::UnsupportedParameter(key_str.to_string()))?;
                let value = args
                    .get(2..)
                    .filter(|rest| !rest.is_empty())
                    .ok_or_else(|| Error::MissingParameter("value".into()))?
                    .join(" ");

                if value.is_empty() {
                    return Err(Error::MissingParameterValue("value cannot be empty".into()));
                }

                Ok(ConfigCommand::Set(key, value))
            }
            Some(cmd) => Err(Error::UnsupportedParameter(cmd.into())),
            None => Ok(ConfigCommand::List),
        }
    }
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
            "/end".to_string(),
            "/new".to_string(),
            "/reload".to_string(),
            "/info".to_string(),
            "/exit".to_string(),
            "/models".to_string(),
            "/config".to_string(),
            "/config set".to_string(),
            "/config get".to_string(),
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
    pub fn parse(input: &str) -> Result<Self> {
        let trimmed = input.trim();

        // Handle config commands
        if trimmed.starts_with("/config") {
            let args: Vec<&str> = trimmed.split_whitespace().skip(1).collect();
            return Ok(Command::Config(ConfigCommand::parse(&args)?));
        }

        match trimmed {
            "/end" => Ok(Command::End),
            "/new" => Ok(Command::New),
            "/reload" => Ok(Command::Reload),
            "/info" => Ok(Command::Info),
            "/exit" => Ok(Command::Exit),
            "/models" => Ok(Command::Models),
            "/retry" => Ok(Command::Retry),
            text => Ok(Command::Message(text.to_string())),
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

    mod config_command {
        use super::*;

        #[test]
        fn parse_empty_args_returns_list() {
            let args: Vec<&str> = vec![];
            let cmd = ConfigCommand::parse(&args).unwrap();
            assert!(matches!(cmd, ConfigCommand::List));
        }

        #[test]
        fn parse_get_command_with_key() {
            let args = vec!["get", "primary-model"];
            let cmd = ConfigCommand::parse(&args).unwrap();
            assert!(matches!(cmd, ConfigCommand::Get(ConfigKey::PrimaryModel)));
        }

        #[test]
        fn parse_get_command_without_key_returns_error() {
            let args = vec!["get"];
            let err = ConfigCommand::parse(&args).unwrap_err();
            assert!(matches!(err, Error::MissingParameter(arg) if arg == "key"));
        }

        #[test]
        fn parse_set_command_with_key_value() {
            let args = vec!["set", "primary-model", "test value with spaces"];
            let cmd = ConfigCommand::parse(&args).unwrap();
            assert!(
                matches!(cmd, ConfigCommand::Set(ConfigKey::PrimaryModel, value) 
                if value == "test value with spaces")
            );
        }

        #[test]
        fn parse_set_command_without_value_returns_error() {
            let args = vec!["set", "primary-model"];
            let err = ConfigCommand::parse(&args).unwrap_err();
            assert!(matches!(err, Error::MissingParameter(arg) if arg == "value"));
        }

        #[test]
        fn parse_set_command_without_key_returns_error() {
            let args = vec!["set"];
            let err = ConfigCommand::parse(&args).unwrap_err();
            assert!(matches!(err, Error::MissingParameter(arg) if arg == "key"));
        }

        #[test]
        fn parse_set_command_with_empty_value_returns_error() {
            let args = vec!["set", "primary-model", ""];
            let err = ConfigCommand::parse(&args).unwrap_err();
            assert!(
                matches!(err, Error::MissingParameterValue(msg) if msg == "value cannot be empty")
            );
        }

        #[test]
        fn parse_invalid_command_returns_error() {
            let args = vec!["invalid"];
            let err = ConfigCommand::parse(&args).unwrap_err();
            assert!(matches!(err, Error::UnsupportedParameter(cmd) if cmd == "invalid"));
        }

        #[test]
        fn parse_set_preserves_value_whitespace() {
            let args = vec![
                "set",
                "primary-model",
                "value",
                "with",
                "  multiple  ",
                "spaces",
            ];
            let cmd = ConfigCommand::parse(&args).unwrap();
            assert!(
                matches!(cmd, ConfigCommand::Set(ConfigKey::PrimaryModel, value) 
                if value == "value with   multiple   spaces")
            );
        }
    }

    mod command_parsing {
        use super::*;

        #[test]
        fn parse_config_list() {
            let result = Command::parse("/config").unwrap();
            assert!(matches!(result, Command::Config(ConfigCommand::List)));
        }

        #[test]
        fn parse_config_get() {
            let result = Command::parse("/config get primary-model").unwrap();
            assert!(matches!(
                result,
                Command::Config(ConfigCommand::Get(ConfigKey::PrimaryModel))
            ));
        }

        #[test]
        fn parse_config_set_single_value() {
            let result = Command::parse("/config set primary-model value").unwrap();
            assert!(
                matches!(result, Command::Config(ConfigCommand::Set(ConfigKey::PrimaryModel, value)) 
                if value == "value")
            );
        }

        #[test]
        fn parse_config_set_multiple_words() {
            let result = Command::parse("/config set primary-model multiple words").unwrap();
            assert!(
                matches!(result, Command::Config(ConfigCommand::Set(ConfigKey::PrimaryModel, value)) 
                if value == "multiple words")
            );
        }
    }
}
