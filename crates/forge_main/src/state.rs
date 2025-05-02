use forge_api::{ConversationId, Model, ModelId, Usage};
use serde::Deserialize;

use crate::prompt::ForgePrompt;

// TODO: convert to a new type
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Plan,
    #[default]
    Act,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Plan => write!(f, "PLAN"),
            Mode::Act => write!(f, "ACT"),
        }
    }
}

//TODO: UIState and ForgePrompt seem like the same thing and can be merged
/// State information for the UI
#[derive(Default, Clone)]
pub struct UIState {
    pub conversation_id: Option<ConversationId>,
    pub usage: Usage,
    pub mode: Mode,
    pub is_first: bool,
    pub model: Option<ModelId>,
    pub cached_models: Option<Vec<Model>>,
    pub estimated_usage: Option<u64>,
}

impl UIState {
    pub fn new(mode: Mode) -> Self {
        Self {
            conversation_id: Default::default(),
            usage: Default::default(),
            mode,
            is_first: true,
            model: Default::default(),
            cached_models: Default::default(),
            estimated_usage: Default::default(),
        }
    }
}

impl From<UIState> for ForgePrompt {
    fn from(state: UIState) -> Self {
        ForgePrompt {
            usage: Some(state.usage),
            mode: state.mode,
            model: state.model,
        }
    }
}
