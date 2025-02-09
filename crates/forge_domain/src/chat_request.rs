use std::path::PathBuf;

use derive_setters::Setters;

use crate::{ConversationId, ModelId};

#[derive(Debug, serde::Deserialize, Clone, Setters)]
#[setters(into, strip_option)]
pub struct ChatRequest {
    pub content: Option<String>,
    pub model: ModelId,
    pub conversation_id: Option<ConversationId>,
    pub custom_instructions: Option<PathBuf>,
}

impl ChatRequest {
    pub fn new(model: ModelId) -> Self {
        Self {
            model,
            content: None,
            conversation_id: None,
            custom_instructions: None,
        }
    }
}
