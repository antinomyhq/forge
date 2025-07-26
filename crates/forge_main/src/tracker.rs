use forge_tracker::{EventKind, PerformanceTimer, ToolCallPayload, ToolType};

use crate::TRACKER;

/// Helper functions to eliminate duplication of tokio::spawn + TRACKER patterns
/// Generic dispatcher for any event
fn dispatch(event: EventKind) {
    tokio::spawn(TRACKER.dispatch(event));
}

/// Dispatches an event blockingly
/// This is useful for events that are not expected to be dispatched in the
/// background
fn dispatch_blocking(event: EventKind) {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(TRACKER.dispatch(event))
    })
    .ok();
}

/// For error events with Debug formatting
pub fn error<E: std::fmt::Debug>(error: E) {
    dispatch(EventKind::Error(format!("{error:?}")));
}

pub fn error_blocking<E: std::fmt::Debug>(error: E) {
    dispatch_blocking(EventKind::Error(format!("{error:?}")));
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
    tokio::spawn(TRACKER.set_model(model));
}

pub fn login(login: String) {
    tokio::spawn(TRACKER.login(login));
}

// Performance tracking functions

/// Track startup phases
pub fn startup_phase(phase: String, duration_ms: u64) {
    tokio::spawn(TRACKER.startup_phase(phase, duration_ms));
}

/// Track service initialization
pub fn service_init(service_name: String, duration_ms: u64) {
    tokio::spawn(TRACKER.service_init(service_name, duration_ms));
}

/// Track tool execution performance
pub fn tool_execution(
    tool_name: String,
    tool_type: ToolType,
    execution_duration_ms: u64,
    queue_duration_ms: u64,
    output_size_bytes: usize,
    error_type: Option<String>,
) {
    tokio::spawn(TRACKER.tool_execution(
        tool_name,
        tool_type,
        execution_duration_ms,
        queue_duration_ms,
        output_size_bytes,
        error_type,
    ));
}

/// Track LLM request performance
pub fn llm_request(
    provider: String,
    model: String,
    request_prep_duration_ms: u64,
    network_duration_ms: u64,
    response_parse_duration_ms: u64,
    total_duration_ms: u64,
    input_tokens: u32,
    output_tokens: u32,
    is_streaming: bool,
) {
    tokio::spawn(TRACKER.llm_request(
        provider,
        model,
        request_prep_duration_ms,
        network_duration_ms,
        response_parse_duration_ms,
        total_duration_ms,
        input_tokens,
        output_tokens,
        is_streaming,
    ));
}

/// Track context operations
pub fn context_operation(
    operation: String,
    duration_ms: u64,
    context_size_messages: usize,
    context_size_tokens: usize,
    memory_usage_bytes: usize,
) {
    tokio::spawn(TRACKER.context_operation(
        operation,
        duration_ms,
        context_size_messages,
        context_size_tokens,
        memory_usage_bytes,
    ));
}

/// Track compaction performance
pub fn compaction(
    trigger_reason: String,
    duration_ms: u64,
    original_messages: usize,
    compacted_messages: usize,
    original_tokens: usize,
    compacted_tokens: usize,
    memory_saved_bytes: usize,
) {
    tokio::spawn(TRACKER.compaction(
        trigger_reason,
        duration_ms,
        original_messages,
        compacted_messages,
        original_tokens,
        compacted_tokens,
        memory_saved_bytes,
    ));
}

/// Track file operations
pub fn file_operation(
    operation: String,
    file_path: String,
    file_size_bytes: usize,
    duration_ms: u64,
    lines_processed: usize,
) {
    tokio::spawn(TRACKER.file_operation(
        operation,
        file_path,
        file_size_bytes,
        duration_ms,
        lines_processed,
    ));
}

/// Track memory snapshots
pub fn memory_snapshot(
    location: String,
    heap_usage_bytes: usize,
    clone_count: usize,
    operation: String,
) {
    tokio::spawn(TRACKER.memory_snapshot(location, heap_usage_bytes, clone_count, operation));
}

/// Create a performance timer for tracking operations
pub fn create_timer(operation: &str) -> PerformanceTimer {
    PerformanceTimer::new(operation)
}
