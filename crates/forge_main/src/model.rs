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

    pub fn command_names(&self) -> Vec<String> {
        self.commands
            .lock()
            .unwrap()
            .iter()
            .map(|command| command.name.clone())
            .collect::<Vec<_>>()
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
        let trimmed = input.trim();
        let is_command = trimmed.starts_with("/");
        if !is_command {
            return Ok(Command::Message(trimmed.to_string()));
        }

        match trimmed {
            "/new" => Ok(Command::New),
            "/info" => Ok(Command::Info),
            "/exit" => Ok(Command::Exit),
            "/dump" => Ok(Command::Dump),
            "/act" => Ok(Command::Act),
            "/plan" => Ok(Command::Plan),
            "/help" => Ok(Command::Help),
            "/model" => Ok(Command::Model),
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
    /// Dumps the current conversation into a json file
    #[strum(props(usage = "Save conversation as JSON"))]
    Dump,
    /// Switch or select the active model
    /// This can be triggered with the '/model' command.
    #[strum(props(usage = "Switch to a different model"))]
    Model,
    /// Handles custom command defined in workflow file.
    Custom(PartialEvent),
}

impl Command {
    pub fn name(&self) -> &str {
        match self {
            Command::New => "/new",
            Command::Message(_) => "/message",
            Command::Info => "/info",
            Command::Exit => "/exit",
            Command::Act => "/act",
            Command::Plan => "/plan",
            Command::Help => "/help",
            Command::Dump => "/dump",
            Command::Model => "/model",
            Command::Custom(event) => &event.name,
        }
    }

    /// Returns the usage description for the command.
    pub fn usage(&self) -> &str {
        self.get_str("usage").unwrap()
    }
}
