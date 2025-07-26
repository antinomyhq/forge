use strum::EnumMessage;
use strum_macros::{Display, EnumIter, EnumMessage as EnumMessageDerive, EnumString};

/// Slash commands for the application
#[derive(Debug, Clone, PartialEq, Eq, Display, EnumString, EnumIter, EnumMessageDerive)]
#[strum(serialize_all = "lowercase")]
pub enum SlashCommand {
    #[strum(message = "Switch between different AI agents.")]
    Agent,

    #[strum(message = "Start new conversation with summarized context")]
    Compact,

    #[strum(message = "Export conversation as JSON or HTML")]
    Dump,

    #[strum(message = "Close the application")]
    Exit,

    #[strum(message = "Switch to agent Forge")]
    Forge,

    #[strum(message = "Access help documentation and instructions")]
    Help,

    #[strum(message = "Display system and environment information")]
    Info,

    #[strum(message = "Authenticate with Forge account")]
    Login,

    #[strum(message = "Sign out from current session")]
    Logout,

    #[strum(message = "Switch to different AI model")]
    Model,

    #[strum(message = "Switch to agent Muse")]
    Muse,

    #[strum(message = "Start new conversation")]
    New,

    #[strum(message = "View available tools")]
    Tools,

    #[strum(message = "Upgrade to latest Forge version")]
    Update,
}

impl SlashCommand {
    /// Get the description of the command
    pub fn description(&self) -> &'static str {
        self.get_message().unwrap_or("No description available")
    }

    /// Perform fuzzy matching on the command name
    /// Returns true if all characters in `query` appear in order within the
    /// command name
    pub fn fuzzy_matches(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }

        let command_name = self.to_string().to_lowercase();
        let query_chars: Vec<char> = query.to_lowercase().chars().collect();
        let command_chars: Vec<char> = command_name.chars().collect();

        let mut query_idx = 0;

        for command_char in command_chars {
            if query_idx < query_chars.len() && command_char == query_chars[query_idx] {
                query_idx += 1;
            }
        }

        query_idx == query_chars.len()
    }

    /// Get all commands that fuzzy match the given query
    pub fn fuzzy_filter(query: &str) -> Vec<SlashCommand> {
        use strum::IntoEnumIterator;

        SlashCommand::iter()
            .filter(|cmd| cmd.fuzzy_matches(query))
            .collect()
    }
}

impl From<SlashCommand> for crate::domain::MenuItem {
    fn from(command: SlashCommand) -> Self {
        crate::domain::MenuItem::new(
            command.to_string(),
            command.description(),
            command.to_string().chars().next().unwrap_or('?'),
        )
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
        let expected = "Switch between different AI agents.";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_enum_iteration() {
        let fixture = SlashCommand::iter().collect::<Vec<_>>();
        let actual = fixture.len();
        let expected = 14;
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
        let expected = "Switch between different AI agents.";
        assert_eq!(actual, expected);
    }
    #[test]
    fn test_fuzzy_matches_exact() {
        let fixture = SlashCommand::Agent;
        let actual = fixture.fuzzy_matches("agent");
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fuzzy_matches_partial() {
        let fixture = SlashCommand::Agent;
        let actual = fixture.fuzzy_matches("ag");
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fuzzy_matches_scattered() {
        let fixture = SlashCommand::Update;
        let actual = fixture.fuzzy_matches("udt");
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fuzzy_matches_case_insensitive() {
        let fixture = SlashCommand::Agent;
        let actual = fixture.fuzzy_matches("AGENT");
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fuzzy_matches_no_match() {
        let fixture = SlashCommand::Agent;
        let actual = fixture.fuzzy_matches("xyz");
        let expected = false;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fuzzy_matches_empty_query() {
        let fixture = SlashCommand::Agent;
        let actual = fixture.fuzzy_matches("");
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fuzzy_filter() {
        let fixture = "ag";
        let actual = SlashCommand::fuzzy_filter(fixture);
        let expected = vec![SlashCommand::Agent];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fuzzy_filter_multiple_matches() {
        let fixture = "e";
        let actual = SlashCommand::fuzzy_filter(fixture);
        // Should match commands that contain 'e': "agent", "exit", "forge", "help",
        // "model", "muse", "new", "update"
        let expected_count = 8;
        assert_eq!(actual.len(), expected_count);
        assert!(actual.contains(&SlashCommand::Help));
        assert!(actual.contains(&SlashCommand::Exit));
        assert!(actual.contains(&SlashCommand::Agent));
        assert!(actual.contains(&SlashCommand::Model));
        assert!(actual.contains(&SlashCommand::Muse));
        assert!(actual.contains(&SlashCommand::Update));
        assert!(actual.contains(&SlashCommand::Forge));
        assert!(actual.contains(&SlashCommand::New));
    }

    #[test]
    fn test_fuzzy_filter_no_matches() {
        let fixture = "xyz";
        let actual = SlashCommand::fuzzy_filter(fixture);
        let expected = vec![];
        assert_eq!(actual, expected);
    }
}
