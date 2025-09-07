use strum::EnumMessage;
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

    #[strum(message = "Enable research mode for systematic codebase exploration and analysis")]
    Sage,

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
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use pretty_assertions::assert_eq;
    use strum::IntoEnumIterator;

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
    fn test_enum_iteration() {
        let fixture = SlashCommand::iter().collect::<Vec<_>>();
        let actual = fixture.len();
        let expected = 13;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sage_command_parsing() {
        let fixture = "sage";
        let actual = SlashCommand::from_str(fixture).unwrap();
        let expected = SlashCommand::Sage;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sage_command_description() {
        let fixture = SlashCommand::Sage;
        let actual = fixture.description();
        let expected = "Enable research mode for systematic codebase exploration and analysis";
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
    }
}
