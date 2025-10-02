use std::path::PathBuf;

use derive_setters::Setters;
use forge_api::{ConversationId, Environment};

//TODO: UIState and ForgePrompt seem like the same thing and can be merged
/// State information for the UI
#[derive(Debug, Default, Clone, Setters)]
#[setters(strip_option)]
pub struct UIState {
    pub cwd: PathBuf,
    pub conversation_id: Option<ConversationId>,
}

impl UIState {
    pub fn new(env: Environment) -> Self {
        Self { cwd: env.cwd, conversation_id: Default::default() }
    }
}
