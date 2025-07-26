use std::time::Duration;

use chrono::{DateTime, Utc};
use edtui::EditorState;
use forge_api::{ChatResponse, ConversationId, Environment};
use throbber_widgets_tui::ThrobberState;
use tui_scrollview::ScrollViewState;

use crate::domain::spotlight::SpotlightState;
use crate::domain::{CancelId, EditorStateExt, Message, Workspace};

#[derive(Clone)]
pub struct State {
    pub workspace: Workspace,
    pub editor: EditorState,
    pub messages: Vec<Message>,
    pub spinner: ThrobberState,
    pub timer: Option<Timer>,
    pub show_spinner: bool,
    pub spotlight: SpotlightState,
    pub conversation: ConversationState,
    pub chat_stream: Option<CancelId>,
    pub message_scroll_state: ScrollViewState,
    pub show_info: bool,
    pub env: Environment,
}

impl State {
    pub fn default(env: Environment) -> Self {
        let prompt_editor = EditorState::default();

        Self {
            workspace: Default::default(),
            editor: prompt_editor,
            messages: Default::default(),
            spinner: Default::default(),
            timer: Default::default(),
            show_spinner: Default::default(),
            spotlight: Default::default(),
            conversation: Default::default(),
            chat_stream: None,
            message_scroll_state: ScrollViewState::default(),
            show_info: false,
            env,
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
