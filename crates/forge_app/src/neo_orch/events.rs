use forge_domain::{
    ChatCompletionMessageFull, ChatResponse, Context, Event, ModelId, TemplateId, ToolCallFull,
    ToolResult,
};

use crate::neo_orch::program::{Identity, SemiGroup};

pub enum UserAction {
    ChatEvent(Event),
    ChatCompletionMessage(anyhow::Result<ChatCompletionMessageFull>),
    ToolResult(ToolResult),
    RenderResult { id: TemplateId, content: String },
}

#[derive(Debug, PartialEq)]
pub enum AgentAction {
    ToolCall {
        call: ToolCallFull,
    },
    Chat {
        model: ModelId,
        context: Context,
    },
    Render {
        id: TemplateId,
        template: String,
        object: serde_json::Value,
    },
    ChatResponse(ChatResponse),
    Combine(Box<AgentAction>, Box<AgentAction>),
    Empty,
}
impl Identity for AgentAction {
    fn identity() -> Self {
        AgentAction::Empty
    }
}

impl SemiGroup for AgentAction {
    fn combine(self, other: Self) -> Self {
        AgentAction::Combine(Box::new(self), Box::new(other))
    }
}
