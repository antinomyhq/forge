use std::path::PathBuf;

use derive_setters::Setters;
use forge_api::{ConversationId, Environment};
use forge_domain::Usage;

/// Metadata for the current conversation including usage tracking.
#[derive(Debug, Default, Clone)]
pub struct ConversationMeta {
    pub id: ConversationId,
    pub accumulated_usage: Usage,
}

impl ConversationMeta {
    /// Creates a new ConversationMeta with the given ID.
    pub fn new(id: ConversationId, usage: Usage) -> Self {
        Self { id, accumulated_usage: usage }
    }

    /// Accumulates usage from a new request.
    pub fn accumulate_usage(&mut self, usage: Usage) {
        self.accumulated_usage = self.accumulated_usage.accumulate(&usage);
    }
}

//TODO: UIState and ForgePrompt seem like the same thing and can be merged
/// State information for the UI
#[derive(Debug, Default, Clone, Setters)]
#[setters(strip_option)]
pub struct UIState {
    pub cwd: PathBuf,
    pub conversation: Option<ConversationMeta>,
}

impl UIState {
    pub fn new(env: Environment) -> Self {
        Self { cwd: env.cwd, conversation: Default::default() }
    }

    /// Returns the current conversation ID if set.
    pub fn conversation_id(&self) -> Option<ConversationId> {
        self.conversation.as_ref().map(|c| c.id)
    }
}
