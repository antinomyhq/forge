use forge_domain::{Context, ModelId, ToolCallFull, ToolResult};

pub enum UserAction {
    Chat(String),
    ToolResult(ToolResult),
    RenderResult(String),
}

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
}

pub struct AgentState {}
