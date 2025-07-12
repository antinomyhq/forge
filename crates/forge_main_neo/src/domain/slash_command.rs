use strum::{EnumMessage, IntoEnumIterator};
use strum_macros::{Display, EnumIter, EnumMessage as EnumMessageDerive, EnumString};

/// Slash commands for the application
#[derive(Debug, Clone, PartialEq, Eq, Display, EnumString, EnumIter, EnumMessageDerive)]
#[strum(serialize_all = "lowercase")]
pub enum SlashCommand {
    #[strum(
        message = "Switch between different AI agents. Use this command to change which agent handles your requests and see available options."
    )]
    Agent,

    #[strum(message = "Compact the conversation context")]
    Compact,

    #[strum(message = "Save conversation as JSON or HTML (use /dump html for HTML format)")]
    Dump,

    #[strum(message = "Exit the application")]
    Exit,

    #[strum(message = "Enable implementation mode with code changes")]
    Forge,

    #[strum(message = "Enable help mode for tool questions")]
    Help,

    #[strum(message = "Display system information")]
    Info,

    #[strum(message = "Switch to a different model")]
    Model,

    #[strum(message = "Enable planning mode without code changes")]
    Muse,

    #[strum(message = "Start a new conversation")]
    New,

    #[strum(message = "List all available tools with their descriptions and schema")]
    Tools,

    #[strum(message = "Updates to the latest compatible version of forge")]
    Update,
}

impl SlashCommand {
    /// Get the description of the command
    pub fn description(&self) -> &'static str {
        self.get_message().unwrap_or("No description available")
    }

    /// Get all available commands with their descriptions
    pub fn all_commands() -> Vec<(SlashCommand, &'static str)> {
        Self::iter()
            .map(|cmd| (cmd.clone(), cmd.description()))
            .collect()
    }

    /// Get the command name with the slash prefix
    pub fn with_slash(&self) -> String {
        format!("/{self}")
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_slash_command_to_string() {
        let fixture = SlashCommand::Agent;
        let actual = fixture.to_string();
        let expected = "agent";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_slash_command_from_string() {
        let fixture = "forge";
        let actual = SlashCommand::from_str(fixture).unwrap();
        let expected = SlashCommand::Forge;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_slash_command_description() {
        let fixture = SlashCommand::Agent;
        let actual = fixture.description();
        let expected = "Switch between different AI agents. Use this command to change which agent handles your requests and see available options.";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_slash_command_with_slash() {
        let fixture = SlashCommand::Help;
        let actual = fixture.with_slash();
        let expected = "/help";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_all_commands_count() {
        let fixture = SlashCommand::all_commands();
        let actual = fixture.len();
        let expected = 12; // Total number of commands
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_all_commands_contains_agent() {
        let fixture = SlashCommand::all_commands();
        let actual = fixture.iter().any(|(cmd, _)| *cmd == SlashCommand::Agent);
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_enum_iteration() {
        let fixture = SlashCommand::iter().collect::<Vec<_>>();
        let actual = fixture.len();
        let expected = 12;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_demonstration_of_slash_command_usage() {
        // Demonstrate parsing from string
        let fixture = "forge";
        let actual = SlashCommand::from_str(fixture).unwrap();
        let expected = SlashCommand::Forge;
        assert_eq!(actual, expected);

        // Demonstrate getting description
        let fixture = SlashCommand::Agent;
        let actual = fixture.description();
        let expected = "Switch between different AI agents. Use this command to change which agent handles your requests and see available options.";
        assert_eq!(actual, expected);

        // Demonstrate listing all commands
        let fixture = SlashCommand::all_commands();
        let actual = fixture.len();
        let expected = 12;
        assert_eq!(actual, expected);

        // Demonstrate getting command with slash
        let fixture = SlashCommand::Help;
        let actual = fixture.with_slash();
        let expected = "/help";
        assert_eq!(actual, expected);
    }
}
