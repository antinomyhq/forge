use std::str::FromStr;

use derive_setters::Setters;
use forge_api::{ConversationId, Model, ModelId, Provider, Usage, Workflow};
use strum_macros::EnumString;

use crate::prompt::ForgePrompt;

#[derive(Debug, Clone, Default, EnumString)]
#[strum(ascii_case_insensitive)]
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
#[derive(Debug, Default, Clone, Setters)]
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
    pub fn new(workflow: Workflow) -> Self {
        let mode = workflow
            .variables
            .get("mode")
            .and_then(|value| value.as_str().and_then(|m| Mode::from_str(m).ok()))
            .unwrap_or_default();
        Self {
            conversation_id: Default::default(),
            usage: Default::default(),
            mode,
            is_first: true,
            model: workflow.model,
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
