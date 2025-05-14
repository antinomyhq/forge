use derive_setters::Setters;
use serde::{Deserialize, Serialize};

use crate::{ConversationId, Event, Mode};

#[derive(Debug, Serialize, Deserialize, Clone, Setters)]
#[setters(into, strip_option)]
pub struct ChatRequest {
    pub event: Event,
    pub conversation_id: ConversationId,
    pub mode: Mode,
}

impl ChatRequest {
    pub fn new(content: Event, conversation_id: ConversationId, mode: Mode) -> Self {
        Self { event: content, conversation_id, mode }
    }
}
