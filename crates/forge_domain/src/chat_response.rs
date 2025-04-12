use serde::Serialize;

use crate::{Event, ToolCallFull, ToolResult, Usage};

/// Events that are emitted by the agent for external consumption. This includes
/// events for all internal state changes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChatResponse {
    Text { text: String, is_complete: bool },
    ToolCallStart(ToolCallFull),
    ToolCallEnd(ToolResult),
    Usage(Usage),
    Event(Event),
}
