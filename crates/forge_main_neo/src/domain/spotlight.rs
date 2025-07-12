use edtui::{EditorMode, EditorState};
use ratatui::widgets::ListState;
use strum::IntoEnumIterator;

use crate::domain::editor_helpers::EditorStateExt;
use crate::domain::slash_command::SlashCommand;

#[derive(Clone)]
pub struct SpotlightState {
    pub is_visible: bool,
    pub editor: EditorState,
    pub selected_index: usize,
    pub list_state: ListState,
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
        }
    }
}

impl SpotlightState {
    /// Get the currently selected command as a SlashCommand enum
    pub fn selected_command(&self) -> Option<SlashCommand> {
        let input_text = self.editor.get_text().to_lowercase();

        // Filter commands that start with the input text
        let filtered_commands: Vec<SlashCommand> = SlashCommand::iter()
            .filter(|cmd| cmd.to_string().to_lowercase().starts_with(&input_text))
            .collect();

        filtered_commands.get(self.selected_index).cloned()
    }

    /// Get all commands that match the current input filter
    pub fn filtered_commands(&self) -> Vec<SlashCommand> {
        let input_text = self.editor.get_text().to_lowercase();

        SlashCommand::iter()
            .filter(|cmd| cmd.to_string().to_lowercase().starts_with(&input_text))
            .collect()
    }
}
