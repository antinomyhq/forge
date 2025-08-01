use edtui::EditorState;
use ratatui::widgets::ListState;

use crate::domain::editor_helpers::EditorStateExt;

#[derive(Clone, Default)]
pub struct AutocompleteState {
    pub suggestions: Vec<String>,
    pub selected_index: usize,
    pub list_state: ListState,
    pub search_term: String,
    pub editor: EditorState,
}

impl AutocompleteState {
    pub fn new(search_term: String) -> Self {
        let mut editor = EditorState::default();
        editor.mode = edtui::EditorMode::Insert;
        editor.set_text_with_cursor_at_end(search_term.clone());

        let mut state = Self {
            suggestions: Vec::new(),
            selected_index: 0,
            list_state: ListState::default(),
            search_term,
            editor,
        };
        state.list_state.select(Some(0));
        state
    }

    pub fn update_search_term(&mut self) {
        self.search_term = self.editor.get_text();
    }
}
