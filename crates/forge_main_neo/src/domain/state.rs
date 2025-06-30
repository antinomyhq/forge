use std::time::Duration;

use chrono::{DateTime, Utc};
use edtui::EditorState;
use forge_api::ChatResponse;
use throbber_widgets_tui::ThrobberState;
use tokio_util::sync::CancellationToken;

use crate::domain::spotlight::SpotlightState;
use crate::domain::{EditorStateExt, Message, Workspace};

#[derive(Clone)]
pub struct State {
    pub workspace: Workspace,
    pub editor: EditorState,
    pub messages: Vec<Message>,
    pub spinner: ThrobberState,
    pub timer: Option<Timer>,
    pub show_spinner: bool,
    pub spotlight: SpotlightState,
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
            spotlight: Default::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Timer {
    pub start_time: DateTime<Utc>,
    pub current_time: DateTime<Utc>,
    pub duration: Duration,
    pub id: TimerId,
}

#[derive(Clone, Debug)]
pub struct TimerId(CancellationToken);

impl TimerId {
    pub fn cancel(&self) {
        self.0.cancel()
    }
}

impl PartialEq for TimerId {
    fn eq(&self, _: &Self) -> bool {
        unimplemented!()
    }
}

impl Eq for TimerId {}

impl From<CancellationToken> for TimerId {
    fn from(value: CancellationToken) -> Self {
        Self(value)
    }
}

impl State {
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
    }

    /// Add an assistant message to the chat
    pub fn add_assistant_message(&mut self, message: ChatResponse) {
        self.messages.push(Message::Assistant(message));
    }
}
