use derive_more::From;
use forge_api::{ChatResponse, ConversationId};
use ratatui::crossterm::event::Event;

use crate::domain::{CancelId, Timer};

/// Top-level application actions that wrap route-specific actions
#[derive(Clone, From, Debug)]
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
    StartStream(tokio_util::sync::CancellationToken),
    Cancelled(CancelId),
}
