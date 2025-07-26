use std::time::Duration;

use chrono::{DateTime, Utc};
use edtui::EditorState;
use forge_api::{ChatResponse, ConversationId};
use throbber_widgets_tui::ThrobberState;
use tui_scrollview::ScrollViewState;

use crate::domain::{CancelId, EditorStateExt, MenuState, Message, SlashCommand, Workspace};

#[derive(Clone)]
pub struct State {
    pub workspace: Workspace,
    pub editor: EditorState,
    pub messages: Vec<Message>,
    pub spinner: ThrobberState,
    pub timer: Option<Timer>,
    pub show_spinner: bool,
    pub conversation: ConversationState,
    pub chat_stream: Option<CancelId>,
    pub message_scroll_state: ScrollViewState,
    pub menu: MenuState,
    pub menu_visible: bool,
}

impl Default for State {
    fn default() -> Self {
        let prompt_editor = EditorState::default();

        Self {
            workspace: Default::default(),
            editor: prompt_editor,
            messages: Default::default(),
            spinner: Default::default(),
            timer: Default::default(),
            show_spinner: Default::default(),
            conversation: Default::default(),
            chat_stream: None,
            message_scroll_state: ScrollViewState::default(),
            menu: MenuState::default(),
            menu_visible: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Timer {
    pub start_time: DateTime<Utc>,
    pub current_time: DateTime<Utc>,
    pub duration: Duration,
    pub cancel: CancelId,
}

impl State {
    /// Determine if the slash menu should be visible based on editor content
    pub fn slash_menu_visible(&self) -> bool {
        self.editor.get_text().starts_with('/')
    }

    /// Update menu visibility based on current state
    pub fn update_menu_visibility(&mut self) {
        use edtui::EditorMode;

        // Menu is visible when:
        // 1. Editor is in normal mode, OR
        // 2. Text starts with "/" (slash command mode) AND there are matching commands
        if self.editor.mode == EditorMode::Normal {
            self.menu_visible = true;
        } else if self.slash_menu_visible() {
            // Check if there are any matching commands for the current search term
            let text = self.editor.get_text();
            let search_term = text.strip_prefix('/').unwrap_or("");
            let filtered_commands = SlashCommand::fuzzy_filter(search_term);
            self.menu_visible = !filtered_commands.is_empty();
        } else {
            self.menu_visible = false;
        }
    }

    /// Get editor lines as strings
    pub fn editor_lines(&self) -> Vec<String> {
        self.editor.get_lines()
    }

    /// Take lines from editor and clear it
    pub fn take_lines(&mut self) -> Vec<String> {
        let text = self.editor_lines();
        self.editor.clear();
        text
    }

    /// Add a user message to the chat
    pub fn add_user_message(&mut self, message: String) {
        self.messages.push(Message::User(message));
        // Auto-scroll to bottom when new message is added
        self.message_scroll_state.scroll_to_bottom();
    }

    /// Add an assistant message to the chat
    pub fn add_assistant_message(&mut self, message: ChatResponse) {
        self.messages.push(Message::Assistant(message));
        // Auto-scroll to bottom when new message is added
        self.message_scroll_state.scroll_to_bottom();
    }
}

#[derive(Clone, Debug, Default)]
pub struct ConversationState {
    pub conversation_id: Option<ConversationId>,
    pub is_first: bool,
}

impl ConversationState {
    pub fn init_conversation(&mut self, conversation_id: ConversationId) {
        self.conversation_id = Some(conversation_id);
        self.is_first = false;
    }
}

#[cfg(test)]
mod tests {
    use edtui::EditorMode;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_menu_visibility_normal_mode() {
        let mut state = State::default();
        state.editor.mode = EditorMode::Normal;

        state.update_menu_visibility();

        assert_eq!(state.menu_visible, true);
    }

    #[test]
    fn test_menu_visibility_insert_mode_no_slash() {
        let mut state = State::default();
        state.editor.mode = EditorMode::Insert;
        state.editor.set_text_insert_mode("hello".to_string());

        state.update_menu_visibility();

        assert_eq!(state.menu_visible, false);
    }

    #[test]
    fn test_menu_visibility_insert_mode_with_slash() {
        let mut state = State::default();
        state.editor.mode = EditorMode::Insert;
        state.editor.set_text_insert_mode("/exit".to_string());

        state.update_menu_visibility();

        assert_eq!(state.menu_visible, true);
    }

    #[test]
    fn test_menu_visibility_insert_mode_just_slash() {
        let mut state = State::default();
        state.editor.mode = EditorMode::Insert;
        state.editor.set_text_insert_mode("/".to_string());

        state.update_menu_visibility();

        assert_eq!(state.menu_visible, true);
    }

    #[test]
    fn test_menu_visibility_insert_mode_no_matching_commands() {
        let mut state = State::default();
        state.editor.mode = EditorMode::Insert;
        state.editor.set_text_insert_mode("/xyz".to_string());

        state.update_menu_visibility();

        assert_eq!(state.menu_visible, false);
    }
}
