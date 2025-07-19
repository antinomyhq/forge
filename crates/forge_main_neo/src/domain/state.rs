use std::time::Duration;

use chrono::{DateTime, Utc};
use edtui::EditorState;
use forge_api::{AgentId, ChatResponse, ConversationId};
use forge_domain::Agent;
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
    pub current_agent: AgentId,
    pub agent_selection: AgentSelectionState,
}

#[derive(Clone, Default, derive_setters::Setters)]
#[setters(strip_option, into)]
pub struct AgentSelectionState {
    pub is_visible: bool,
    pub available_agents: Vec<Agent>,
    pub selected_index: usize,
    pub editor: EditorState,
    pub current_agent: AgentId,
}

impl std::fmt::Debug for AgentSelectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentSelectionState")
            .field("is_visible", &self.is_visible)
            .field("available_agents", &self.available_agents)
            .field("selected_index", &self.selected_index)
            .field("editor", &"<EditorState>")
            .field("current_agent", &self.current_agent)
            .finish()
    }
}

impl AgentSelectionState {
    /// Get all agents that match the current search filter
    pub fn filtered_agents(&self) -> Vec<&Agent> {
        let search_text = self.editor.get_text().to_lowercase();

        if search_text.is_empty() {
            self.available_agents.iter().collect()
        } else {
            self.available_agents
                .iter()
                .filter(|agent| {
                    // Search in agent ID
                    agent.id.as_str().to_lowercase().contains(&search_text) ||
                    // Search in agent title
                    agent.title.as_ref()
                        .map(|title| title.to_lowercase().contains(&search_text))
                        .unwrap_or(false)
                })
                .collect()
        }
    }

    /// Get the currently selected agent from filtered results
    pub fn selected_agent(&self) -> Option<&Agent> {
        let filtered = self.filtered_agents();
        filtered.get(self.selected_index).copied()
    }
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
            conversation: Default::default(),
            chat_stream: None,
            message_scroll_state: ScrollViewState::default(),
            current_agent: AgentId::FORGE,
            agent_selection: AgentSelectionState {
                current_agent: AgentId::FORGE,
                ..Default::default()
            },
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
