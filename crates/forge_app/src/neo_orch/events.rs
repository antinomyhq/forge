use forge_domain::{
    ChatCompletionMessageFull, ChatResponse, Context, Event, ModelId, TemplateId, ToolCallFull,
    ToolResult,
};

use crate::neo_orch::program::{Identity, SemiGroup};

pub enum AgentAction {
    ChatEvent(Event),
    ChatCompletionMessage(anyhow::Result<ChatCompletionMessageFull>),
    ToolResult(ToolResult),
    RenderResult { id: TemplateId, content: String },
}

#[derive(Debug, PartialEq)]
pub enum AgentCommand {
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
    Combine(Box<AgentCommand>, Box<AgentCommand>),
    Empty,
}
impl Identity for AgentCommand {
    fn identity() -> Self {
        AgentCommand::Empty
    }
}

impl SemiGroup for AgentCommand {
    fn combine(self, other: Self) -> Self {
        AgentCommand::Combine(Box::new(self), Box::new(other))
    }
}
