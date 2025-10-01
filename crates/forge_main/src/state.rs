use std::path::PathBuf;

use derive_setters::Setters;
use forge_api::{AgentId, ConversationId, Environment, ModelId, Provider, Usage};

use crate::prompt::ForgePrompt;

//TODO: UIState and ForgePrompt seem like the same thing and can be merged
/// State information for the UI
#[derive(Debug, Default, Clone, Setters)]
#[setters(strip_option)]
pub struct UIState {
    pub cwd: PathBuf,
    pub conversation_id: Option<ConversationId>,
    pub usage: Usage,
    pub operating_agent: AgentId,
    pub is_first: bool,
    pub model: Option<ModelId>,
    pub provider: Option<Provider>,
}

impl UIState {
    pub fn new(env: Environment, operating_agent: AgentId, model: Option<ModelId>) -> Self {
        Self {
            cwd: env.cwd,
            conversation_id: Default::default(),
            usage: Default::default(),
            is_first: true,
            model,
            operating_agent,
            provider: Default::default(),
        }
    }
}

impl From<UIState> for ForgePrompt {
    fn from(state: UIState) -> Self {
        ForgePrompt {
            cwd: state.cwd,
            usage: Some(state.usage),
            model: state.model,
            agent_id: state.operating_agent,
        }
    }
}
