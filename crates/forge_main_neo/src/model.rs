use derive_more::From;
use edtui::EditorState;
use ratatui::crossterm::event::{KeyEvent, MouseEvent};

#[derive(Default)]
pub struct State {
    pub messages: Vec<String>,
    pub editor: EditorState,
    pub exit: bool,
}

#[derive(From)]
pub enum Action {
    KeyEvent(KeyEvent),
    MouseEvent(MouseEvent),
}
