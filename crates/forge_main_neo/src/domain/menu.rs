use convert_case::{Case, Casing};
use ratatui::widgets::ListState;

#[derive(Clone, Debug)]
pub struct MenuState {
    pub list: ListState,
}

impl Default for MenuState {
    fn default() -> Self {
        let mut list = ListState::default();
        list.select(Some(0)); // Start with first item selected
        Self { list }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MenuItem {
    pub title: String,
    pub description: String,
    pub shortcut: char,
}

impl MenuItem {
    pub fn new(
        title: impl Into<String>,
        description: impl Into<String>,
        trigger_letter: char,
    ) -> Self {
        Self {
            title: title.into().to_case(Case::Title),
            description: description.into(),
            shortcut: trigger_letter,
        }
    }
}
