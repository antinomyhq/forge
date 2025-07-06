use forge_tracker::{EventKind, ToolCallPayload};

use crate::TRACKER;

/// Helper functions to eliminate duplication of tokio::spawn + TRACKER patterns
/// Generic dispatcher for any event
pub fn dispatch(event: EventKind) {
    std::mem::drop(tokio::spawn(async move { TRACKER.dispatch(event).await }));
}

/// For error events with Debug formatting
pub fn error<E: std::fmt::Debug>(error: E) {
    dispatch(EventKind::Error(format!("{error:?}")));
}

/// For error events with string input
pub fn error_string(error: String) {
    dispatch(EventKind::Error(error));
}

/// For tool call events
pub fn tool_call(payload: ToolCallPayload) {
    dispatch(EventKind::ToolCall(payload));
}

/// For prompt events
pub fn prompt(text: String) {
    dispatch(EventKind::Prompt(text));
}

/// For model setting
pub fn set_model(model: String) {
    std::mem::drop(tokio::spawn(async move { TRACKER.set_model(model).await }));
}

pub(crate) fn login(login: String) {
    dispatch(EventKind::Login(login));
}
