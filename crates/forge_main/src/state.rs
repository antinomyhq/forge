use std::path::PathBuf;

use derive_setters::Setters;
use forge_api::{ConversationId, Environment, Provider, Usage};

//TODO: UIState and ForgePrompt seem like the same thing and can be merged
/// State information for the UI
#[derive(Debug, Default, Clone, Setters)]
#[setters(strip_option)]
pub struct UIState {
    pub cwd: PathBuf,
    pub conversation_id: Option<ConversationId>,
    pub usage: Usage,
    pub is_first: bool,
    pub provider: Option<Provider>,
}

impl UIState {
    pub fn new(env: Environment) -> Self {
        Self {
            cwd: env.cwd,
            conversation_id: Default::default(),
            usage: Default::default(),
            is_first: true,
            provider: Default::default(),
        }
    }
}
