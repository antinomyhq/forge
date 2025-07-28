use ratatui::widgets::ListState;

#[derive(Clone, Debug, Default)]
pub struct AutocompleteState {
    pub suggestions: Vec<String>,
    pub selected_index: usize,
    pub list_state: ListState,
}