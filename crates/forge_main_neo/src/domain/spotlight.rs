use edtui::{EditorMode, EditorState};
use ratatui::widgets::ListState;
use strum::IntoEnumIterator;

use crate::domain::editor_helpers::EditorStateExt;
use crate::domain::forge_config::CustomCommand;
use crate::domain::slash_command::SlashCommand;

/// Represents all available commands in spotlight
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpotlightCommand {
    Slash(SlashCommand),
    Custom(CustomCommand),
}

impl SpotlightCommand {
    /// Get the command name
    pub fn name(&self) -> String {
        match self {
            SpotlightCommand::Slash(cmd) => cmd.to_string(),
            SpotlightCommand::Custom(cmd) => cmd.name.clone(),
        }
    }

    /// Get the command description
    pub fn description(&self) -> String {
        match self {
            SpotlightCommand::Slash(cmd) => cmd.description().to_string(),
            SpotlightCommand::Custom(cmd) => cmd.description.clone(),
        }
    }
}

#[derive(Clone)]
pub struct SpotlightState {
    pub is_visible: bool,
    pub editor: EditorState,
    pub selected_index: usize,
    pub list_state: ListState,
    pub custom_commands: Vec<CustomCommand>,
}

impl Default for SpotlightState {
    fn default() -> Self {
        let mut editor = EditorState::default();
        editor.mode = EditorMode::Insert;

        Self {
            is_visible: false,
            editor,
            selected_index: 0,
            list_state: ListState::default(),
            custom_commands: Vec::new(),
        }
    }
}

impl SpotlightState {
    /// Set custom commands
    pub fn set_custom_commands(&mut self, commands: Vec<CustomCommand>) {
        self.custom_commands = commands;
    }
    /// Reset spotlight state while preserving custom commands
    pub fn reset(&mut self) {
        let custom_commands = self.custom_commands.clone();
        *self = Self::default();
        self.custom_commands = custom_commands;
    }

    /// Get all available commands (slash + custom)
    pub fn all_commands(&self) -> Vec<SpotlightCommand> {
        let mut commands = Vec::new();

        // Add slash commands
        for cmd in SlashCommand::iter() {
            commands.push(SpotlightCommand::Slash(cmd));
        }

        // Add custom commands
        for cmd in &self.custom_commands {
            commands.push(SpotlightCommand::Custom(cmd.clone()));
        }

        commands
    }

    /// Get the currently selected command as a SpotlightCommand
    pub fn selected_command(&self) -> Option<SpotlightCommand> {
        let filtered_commands = self.filtered_commands();
        filtered_commands.get(self.selected_index).cloned()
    }

    /// Get all commands that match the current input filter
    pub fn filtered_commands(&self) -> Vec<SpotlightCommand> {
        let input_text = self.editor.get_text().to_lowercase();

        self.all_commands()
            .into_iter()
            .filter(|cmd| cmd.name().to_lowercase().starts_with(&input_text))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_spotlight_state_reset_preserves_custom_commands() {
        let fixture_commands = vec![
            CustomCommand::new("test1", "Test 1", "Prompt 1"),
            CustomCommand::new("test2", "Test 2", "Prompt 2"),
        ];
        let mut fixture_state = SpotlightState::default();
        fixture_state.set_custom_commands(fixture_commands.clone());
        fixture_state.is_visible = true;
        fixture_state.selected_index = 5;

        fixture_state.reset();

        let actual_commands = fixture_state.custom_commands;
        let expected_commands = fixture_commands;
        assert_eq!(actual_commands, expected_commands);

        let actual_visible = fixture_state.is_visible;
        let expected_visible = false;
        assert_eq!(actual_visible, expected_visible);

        let actual_index = fixture_state.selected_index;
        let expected_index = 0;
        assert_eq!(actual_index, expected_index);
    }
    #[test]
    fn test_spotlight_custom_commands_persist_after_reset() {
        let fixture_commands = vec![
            CustomCommand::new(
                "deploy",
                "Deploy application",
                "Deploy the app to production",
            ),
            CustomCommand::new("test", "Run tests", "Execute all test suites"),
        ];
        let mut fixture_state = SpotlightState::default();
        fixture_state.set_custom_commands(fixture_commands.clone());

        // Verify custom commands are initially available
        let initial_commands = fixture_state.all_commands();
        let custom_command_count = initial_commands
            .iter()
            .filter(|cmd| matches!(cmd, SpotlightCommand::Custom(_)))
            .count();
        assert_eq!(custom_command_count, 2);

        // Reset the spotlight state (simulating closing and reopening)
        fixture_state.reset();

        // Verify custom commands are still available after reset
        let actual_commands = fixture_state.all_commands();
        let actual_custom_count = actual_commands
            .iter()
            .filter(|cmd| matches!(cmd, SpotlightCommand::Custom(_)))
            .count();
        let expected_custom_count = 2;
        assert_eq!(actual_custom_count, expected_custom_count);

        // Verify the custom commands are the same ones we set
        let actual_custom_commands: Vec<_> = actual_commands
            .iter()
            .filter_map(|cmd| match cmd {
                SpotlightCommand::Custom(custom_cmd) => Some(custom_cmd),
                _ => None,
            })
            .collect();

        assert_eq!(actual_custom_commands.len(), 2);
        assert!(
            actual_custom_commands
                .iter()
                .any(|cmd| cmd.name == "deploy")
        );
        assert!(actual_custom_commands.iter().any(|cmd| cmd.name == "test"));
    }
}
