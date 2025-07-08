use forge_tracker::{EventKind, ToolCallPayload};

use crate::TRACKER;

/// Helper functions to eliminate duplication of tokio::spawn + TRACKER patterns
/// Generic dispatcher for any event
fn dispatch(event: EventKind) {
    std::mem::drop(tokio::spawn(async move { TRACKER.dispatch(event).await }));
}

/// Dispatches an event blockingly
/// This is useful for events that are not expected to be dispatched in the
/// background
fn dispatch_async(event: EventKind) {
    let result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(TRACKER.dispatch(event))
    });

    match result {
        Ok(()) => {
            println!("Event dispatched successfully");
        }
        Err(e) => {
            println!("Failed to dispatch event");
        }
    }
}

/// For error events with Debug formatting
pub fn error<E: std::fmt::Debug>(error: E) {
    dispatch_async(EventKind::Error(format!("{error:?}")));
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

pub fn login(login: String) {
    std::mem::drop(tokio::spawn(TRACKER.login(login)));
}
