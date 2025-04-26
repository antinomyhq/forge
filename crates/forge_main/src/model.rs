use std::sync::{Arc, Mutex};

use forge_api::{Model, Workflow};
use strum::{EnumProperty, IntoEnumIterator};
use strum_macros::{EnumIter, EnumProperty};

use crate::info::Info;
use crate::ui::PartialEvent;

fn humanize_context_length(length: u64) -> String {
    if length >= 1_000_000 {
        format!("{:.1}M context", length as f64 / 1_000_000.0)
    } else if length >= 1_000 {
        format!("{:.1}K context", length as f64 / 1_000.0)
    } else {
        format!("{length} context")
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeCommand {
    pub name: String,
    pub description: String,
    pub value: Option<String>,
}

impl From<&Workflow> for ForgeCommandManager {
    fn from(value: &Workflow) -> Self {
        let cmd = ForgeCommandManager::default();
        cmd.register_all(value);
        cmd
    }
}

#[derive(Debug)]
pub struct ForgeCommandManager {
    commands: Arc<Mutex<Vec<ForgeCommand>>>,
}

impl Default for ForgeCommandManager {
    fn default() -> Self {
        let commands = Self::default_commands();
        ForgeCommandManager { commands: Arc::new(Mutex::new(commands)) }
    }
}

impl ForgeCommandManager {
    fn default_commands() -> Vec<ForgeCommand> {
        Command::iter()
            .filter(|command| !matches!(command, Command::Message(_)))
            .filter(|command| !matches!(command, Command::Custom(_)))
            .filter(|command| !matches!(command, Command::Shell(_)))
            .map(|command| ForgeCommand {
                name: command.name().to_string(),
                description: command.usage().to_string(),
                value: None,
            })
            .collect::<Vec<_>>()
    }

    /// Registers multiple commands to the manager.
    pub fn register_all(&self, workflow: &Workflow) {
        let mut guard = self.commands.lock().unwrap();
        let mut commands = Self::default_commands();

        commands.sort_by(|a, b| a.name.cmp(&b.name));

        commands.extend(workflow.commands.clone().into_iter().map(|cmd| {
            let name = format!("/{}", cmd.name);
            let description = format!("⚙ {}", cmd.description);
            let value = cmd.prompt.clone();

            ForgeCommand { name, description, value }
        }));

        *guard = commands;
    }

    /// Finds a command by name.
    fn find(&self, command: &str) -> Option<ForgeCommand> {
        self.commands
            .lock()
            .unwrap()
            .iter()
            .find(|c| c.name == command)
            .cloned()
    }

    /// Lists all registered commands.
    pub fn list(&self) -> Vec<ForgeCommand> {
        self.commands.lock().unwrap().clone()
    }

    /// Extracts the command value from the input parts
    ///
    /// # Arguments
    /// * `command` - The command for which to extract the value
    /// * `parts` - The parts of the command input after the command name
    ///
    /// # Returns
    /// * `Option<String>` - The extracted value, if any
    fn extract_command_value(&self, command: &ForgeCommand, parts: &[&str]) -> Option<String> {
        // Unit tests implemented in the test module below

        // Try to get value provided in the command
        let value_provided = if !parts.is_empty() {
            Some(parts.join(" "))
        } else {
            None
        };

        // Try to get default value from command definition
        let value_default = self
            .commands
            .lock()
            .unwrap()
            .iter()
            .find(|c| c.name == command.name)
            .and_then(|cmd| cmd.value.clone());

        // Use provided value if non-empty, otherwise use default
        match value_provided {
            Some(value) if !value.trim().is_empty() => Some(value),
            _ => value_default,
        }
    }

    pub fn parse(&self, input: &str) -> anyhow::Result<Command> {
        // Check if it's a shell command (starts with !)
        if input.trim().starts_with("!") {
            return Ok(Command::Shell(
                input
                    .strip_prefix("!")
                    .unwrap_or_default()
                    .trim()
                    .to_string(),
            ));
        }

        let mut tokens = input.trim().split_ascii_whitespace();
        let command = tokens.next().unwrap();
        let parameters = tokens.collect::<Vec<_>>();

        // Check if it's a system command (starts with /)
        let is_command = command.starts_with("/");
        if !is_command {
            return Ok(Command::Message(input.to_string()));
        }

        // TODO: Can leverage Clap to parse commands and provide correct error messages
        match command {
            "/compact" => Ok(Command::Compact),
            "/new" => Ok(Command::New),
            "/info" => Ok(Command::Info),
            "/exit" => Ok(Command::Exit),
            "/dump" => {
                if !parameters.is_empty() && parameters[0] == "html" {
                    Ok(Command::Dump(Some("html".to_string())))
                } else {
                    Ok(Command::Dump(None))
                }
            }
            "/act" => Ok(Command::Act),
            "/plan" => Ok(Command::Plan),
            "/help" => Ok(Command::Help),
            "/model" => Ok(Command::Model),
            "/tools" => Ok(Command::Tools),
            text => {
                let parts = text.split_ascii_whitespace().collect::<Vec<&str>>();

                if let Some(command) = parts.first() {
                    if let Some(command) = self.find(command) {
                        let value = self.extract_command_value(&command, &parts[1..]);

                        Ok(Command::Custom(PartialEvent::new(
                            command.name.clone().strip_prefix('/').unwrap().to_string(),
                            value.unwrap_or_default(),
                        )))
                    } else {
                        Err(anyhow::anyhow!("{} is not valid", command))
                    }
                } else {
                    Err(anyhow::anyhow!("Invalid Command Format."))
                }
            }
        }
    }
}

/// Represents user input types in the chat application.
///
/// This enum encapsulates all forms of input including:
/// - System commands (starting with '/')
/// - Regular chat messages
/// - File content
#[derive(Debug, Clone, PartialEq, Eq, EnumProperty, EnumIter)]
pub enum Command {
    /// Compact the conversation context. This can be triggered with the
    /// '/compact' command.
    #[strum(props(usage = "Compact the conversation context"))]
    Compact,
    /// Start a new conversation while preserving history.
    /// This can be triggered with the '/new' command.
    #[strum(props(usage = "Start a new conversation"))]
    New,
    /// A regular text message from the user to be processed by the chat system.
    /// Any input that doesn't start with '/' is treated as a message.
    #[strum(props(usage = "Send a regular message"))]
    Message(String),
    /// Display system environment information.
    /// This can be triggered with the '/info' command.
    #[strum(props(usage = "Display system information"))]
    Info,
    /// Exit the application without any further action.
    #[strum(props(usage = "Exit the application"))]
    Exit,
    /// Switch to "act" mode.
    /// This can be triggered with the '/act' command.
    #[strum(props(usage = "Enable implementation mode with code changes"))]
    Act,
    /// Switch to "plan" mode.
    /// This can be triggered with the '/plan' command.
    #[strum(props(usage = "Enable planning mode without code changes"))]
    Plan,
    /// Switch to "help" mode.
    /// This can be triggered with the '/help' command.
    #[strum(props(usage = "Enable help mode for tool questions"))]
    Help,
    /// Dumps the current conversation into a json file or html file
    #[strum(props(usage = "Save conversation as JSON or HTML (use /dump html for HTML format)"))]
    Dump(Option<String>),
    /// Switch or select the active model
    /// This can be triggered with the '/model' command.
    #[strum(props(usage = "Switch to a different model"))]
    Model,
    /// List all available tools with their descriptions and schema
    /// This can be triggered with the '/tools' command.
    #[strum(props(usage = "List all available tools with their descriptions and schema"))]
    Tools,
    /// Handles custom command defined in workflow file.
    Custom(PartialEvent),
    /// Executes a native shell command.
    /// This can be triggered with commands starting with '!' character.
    #[strum(props(usage = "Execute a native shell command"))]
    Shell(String),
}

impl Command {
    pub fn name(&self) -> &str {
        match self {
            Command::Compact => "/compact",
            Command::New => "/new",
            Command::Message(_) => "/message",
            Command::Info => "/info",
            Command::Exit => "/exit",
            Command::Act => "/act",
            Command::Plan => "/plan",
            Command::Help => "/help",
            Command::Dump(_) => "/dump",
            Command::Model => "/model",
            Command::Tools => "/tools",
            Command::Custom(event) => &event.name,
            Command::Shell(_) => "!shell",
        }
    }

    /// Returns the usage description for the command.
    pub fn usage(&self) -> &str {
        self.get_str("usage").unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_command_value_with_provided_value() {
        // Setup
        let cmd_manager = ForgeCommandManager::default();
        let command = ForgeCommand {
            name: String::from("/test"),
            description: String::from("Test command"),
            value: None,
        };
        let parts = vec!["arg1", "arg2"];

        // Execute
        let result = cmd_manager.extract_command_value(&command, &parts);

        // Verify
        assert_eq!(result, Some(String::from("arg1 arg2")));
    }

    #[test]
    fn test_extract_command_value_with_empty_parts_default_value() {
        // Setup
        let cmd_manager = ForgeCommandManager {
            commands: Arc::new(Mutex::new(vec![ForgeCommand {
                name: String::from("/test"),
                description: String::from("Test command"),
                value: Some(String::from("default_value")),
            }])),
        };
        let command = ForgeCommand {
            name: String::from("/test"),
            description: String::from("Test command"),
            value: None,
        };
        let parts: Vec<&str> = vec![];

        // Execute
        let result = cmd_manager.extract_command_value(&command, &parts);

        // Verify
        assert_eq!(result, Some(String::from("default_value")));
    }

    #[test]
    fn test_extract_command_value_with_empty_string_parts() {
        // Setup
        let cmd_manager = ForgeCommandManager {
            commands: Arc::new(Mutex::new(vec![ForgeCommand {
                name: String::from("/test"),
                description: String::from("Test command"),
                value: Some(String::from("default_value")),
            }])),
        };
        let command = ForgeCommand {
            name: String::from("/test"),
            description: String::from("Test command"),
            value: None,
        };
        let parts = vec![""];

        // Execute
        let result = cmd_manager.extract_command_value(&command, &parts);

        // Verify - should use default as the provided value is empty
        assert_eq!(result, Some(String::from("default_value")));
    }

    #[test]
    fn test_extract_command_value_with_whitespace_parts() {
        // Setup
        let cmd_manager = ForgeCommandManager {
            commands: Arc::new(Mutex::new(vec![ForgeCommand {
                name: String::from("/test"),
                description: String::from("Test command"),
                value: Some(String::from("default_value")),
            }])),
        };
        let command = ForgeCommand {
            name: String::from("/test"),
            description: String::from("Test command"),
            value: None,
        };
        let parts = vec!["  "];

        // Execute
        let result = cmd_manager.extract_command_value(&command, &parts);

        // Verify - should use default as the provided value is just whitespace
        assert_eq!(result, Some(String::from("default_value")));
    }

    #[test]
    fn test_extract_command_value_no_default_no_provided() {
        // Setup
        let cmd_manager = ForgeCommandManager {
            commands: Arc::new(Mutex::new(vec![ForgeCommand {
                name: String::from("/test"),
                description: String::from("Test command"),
                value: None,
            }])),
        };
        let command = ForgeCommand {
            name: String::from("/test"),
            description: String::from("Test command"),
            value: None,
        };
        let parts: Vec<&str> = vec![];

        // Execute
        let result = cmd_manager.extract_command_value(&command, &parts);

        // Verify - should be None as there's no default and no provided value
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_command_value_provided_overrides_default() {
        // Setup
        let cmd_manager = ForgeCommandManager {
            commands: Arc::new(Mutex::new(vec![ForgeCommand {
                name: String::from("/test"),
                description: String::from("Test command"),
                value: Some(String::from("default_value")),
            }])),
        };
        let command = ForgeCommand {
            name: String::from("/test"),
            description: String::from("Test command"),
            value: None,
        };
        let parts = vec!["provided_value"];

        // Execute
        let result = cmd_manager.extract_command_value(&command, &parts);

        // Verify - provided value should override default
        assert_eq!(result, Some(String::from("provided_value")));
    }
    #[test]
    fn test_parse_shell_command() {
        // Setup
        let cmd_manager = ForgeCommandManager::default();

        // Execute
        let result = cmd_manager.parse("!ls -la").unwrap();

        // Verify
        match result {
            Command::Shell(cmd) => assert_eq!(cmd, "ls -la"),
            _ => panic!("Expected Shell command, got {result:?}"),
        }
    }

    #[test]
    fn test_parse_shell_command_empty() {
        // Setup
        let cmd_manager = ForgeCommandManager::default();

        // Execute
        let result = cmd_manager.parse("!").unwrap();

        // Verify
        match result {
            Command::Shell(cmd) => assert_eq!(cmd, ""),
            _ => panic!("Expected Shell command, got {result:?}"),
        }
    }

    #[test]
    fn test_parse_shell_command_with_whitespace() {
        // Setup
        let cmd_manager = ForgeCommandManager::default();

        // Execute
        let result = cmd_manager.parse("!   echo 'test'   ").unwrap();

        // Verify
        match result {
            Command::Shell(cmd) => assert_eq!(cmd, "echo 'test'"),
            _ => panic!("Expected Shell command, got {result:?}"),
        }
    }

    #[test]
    fn test_shell_command_not_in_default_commands() {
        // Setup
        let manager = ForgeCommandManager::default();
        let commands = manager.list();

        // The shell command should not be included
        let contains_shell = commands.iter().any(|cmd| cmd.name == "!shell");
        assert!(
            !contains_shell,
            "Shell command should not be in default commands"
        );
    }
}
