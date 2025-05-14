use derive_setters::Setters;
use forge_api::{ConversationId, Mode, Model, ModelId, Provider, Usage};

use crate::prompt::ForgePrompt;

//TODO: UIState and ForgePrompt seem like the same thing and can be merged
/// State information for the UI
#[derive(Default, Clone, Setters)]
#[setters(strip_option)]
pub struct UIState {
    pub conversation_id: Option<ConversationId>,
    pub usage: Usage,
    pub mode: Mode,
    pub is_first: bool,
    pub model: Option<ModelId>,
    pub cached_models: Option<Vec<Model>>,
    pub provider: Option<Provider>,
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
            provider: Default::default(),
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
