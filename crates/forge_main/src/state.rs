use forge_api::{ConversationId, Usage};

use crate::input::PromptInput;

#[derive(Clone, Default)]
pub enum Mode {
    Plan,
    Help,
    #[default]
    Act,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Plan => write!(f, "PLAN"),
            Mode::Help => write!(f, "HELP"),
            Mode::Act => write!(f, "ACT"),
        }
    }
}

/// State information for the UI
pub struct UIState {
    pub current_title: Option<String>,
    pub conversation_id: Option<ConversationId>,
    pub usage: Usage,
    pub mode: Mode,
    pub is_first: bool,
}

impl Default for UIState {
    fn default() -> Self {
        Self {
            current_title: Default::default(),
            conversation_id: Default::default(),
            usage: Default::default(),
            mode: Default::default(),
            is_first: true,
        }
    }
}

impl From<&UIState> for PromptInput {
    fn from(state: &UIState) -> Self {
        PromptInput::Update {
            title: state.current_title.clone(),
            usage: Some(state.usage.clone()),
            mode: state.mode.clone(),
        }
    }
}
