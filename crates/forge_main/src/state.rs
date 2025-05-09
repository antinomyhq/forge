use derive_setters::Setters;
use forge_api::{ConversationId, Model, ModelId, Provider, Usage};
use serde::Deserialize;

use crate::prompt::ForgePrompt;

// TODO: convert to a new type
#[derive(Debug, Clone, Default)]
pub enum Mode {
    Plan,
    #[default]
    Act,
}

// Implement a custom deserializer for case-insensitive matching
impl<'de> Deserialize<'de> for Mode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?.to_lowercase();
        match s.as_str() {
            "plan" => Ok(Mode::Plan),
            "act" => Ok(Mode::Act),
            _ => Err(serde::de::Error::custom(format!(
                "Unknown Mode variant: {s}, expected to be one of 'plan', 'act'"
            ))),
        }
    }
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
