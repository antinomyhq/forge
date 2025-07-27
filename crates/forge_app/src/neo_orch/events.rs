use forge_domain::{
    ChatCompletionMessage, ChatResponse, Context, ModelId, ToolCallFull, ToolResult,
};

use crate::neo_orch::program::{Identity, SemiGroup};

pub enum UserAction {
    Message(String),
    ChatCompletionMessage(anyhow::Result<ChatCompletionMessage>),
    ToolResult(ToolResult),
    RenderResult(String),
}

#[derive(Debug)]
pub enum AgentAction {
    ToolCall {
        call: ToolCallFull,
    },
    Chat {
        model: ModelId,
        context: Context,
    },
    Render {
        template: &'static str,
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
