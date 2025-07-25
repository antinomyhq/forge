use forge_api::{ChatResponse, ConversationId};
use ratatui::crossterm::event::Event;

use crate::domain::{CancelId, Timer};

/// Top-level application actions that wrap route-specific actions
#[derive(Clone, Debug)]
pub enum Action {
    CrossTerm(Event),
    Initialize,
    Workspace {
        current_dir: Option<String>,
        current_branch: Option<String>,
    },
    ChatResponse(ChatResponse),
    ConversationInitialized(ConversationId),
    IntervalTick(Timer),
    InterruptStream,
    StartStream(CancelId),
    ShellCommandResult {
        command: String,
        output: String,
        success: bool,
    },
}
