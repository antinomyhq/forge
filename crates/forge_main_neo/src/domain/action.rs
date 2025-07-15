use forge_api::{ChatResponse, ConversationId, Model, ModelId};
use ratatui::crossterm::event::Event;

use crate::domain::{CancelId, Timer};

/// Top-level application actions that wrap route-specific actions
#[derive(Clone, Debug)]
pub enum Action {
    CrossTerm(Event),
    Initialize,
    WorkflowLoaded(ModelId),
    Workspace {
        current_dir: Option<String>,
        current_branch: Option<String>,
    },
    ChatResponse(ChatResponse),
    ConversationInitialized(ConversationId),
    IntervalTick(Timer),
    InterruptStream,
    StartStream(CancelId),
    ShowModelSelection,
    ModelsLoaded(Vec<Model>),
    ModelSelected(ModelId),
}
