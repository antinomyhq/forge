use std::path::PathBuf;
use std::sync::Arc;

use derive_setters::Setters;
use serde::Serialize;

use crate::{ConversationId, Event, UserResponse};

#[derive(Serialize, Clone, Setters)]
#[setters(into, strip_option)]
pub struct ChatRequest {
    pub event: Event,
    pub conversation_id: ConversationId,
    pub workflow_path: PathBuf,
    #[serde(skip)]
    pub confirm_fn: Arc<dyn Fn() -> UserResponse + Send + Sync>,
}

impl std::fmt::Debug for ChatRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatRequest")
            .field("event", &self.event)
            .field("conversation_id", &self.conversation_id)
            .field("workflow_path", &self.workflow_path)
            .field("confirm_fn", &"<function>")
            .finish()
    }
}

impl ChatRequest {
    pub fn new(
        content: Event,
        conversation_id: ConversationId,
        workflow_path: PathBuf,
        confirm_fn: Arc<dyn Fn() -> UserResponse + Send + Sync>,
    ) -> Self {
        Self { event: content, conversation_id, workflow_path, confirm_fn }
    }
}
