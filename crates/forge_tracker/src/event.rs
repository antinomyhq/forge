use std::ops::Deref;

use chrono::{DateTime, Utc};
use convert_case::{Case, Casing};
use forge_domain::Conversation;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub event_name: Name,
    pub event_value: String,
    pub start_time: DateTime<Utc>,
    pub cores: usize,
    pub client_id: String,
    pub os_name: String,
    pub up_time: i64,
    pub path: Option<String>,
    pub cwd: Option<String>,
    pub user: String,
    pub args: Vec<String>,
    pub version: String,
    pub email: Vec<String>,
    pub model: Option<String>,
    pub conversation: Option<Conversation>,
    pub identity: Option<Identity>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Identity {
    pub login: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Name(String);
impl From<String> for Name {
    fn from(name: String) -> Self {
        Self(name.to_case(Case::Snake))
    }
}
impl Deref for Name {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Name> for String {
    fn from(val: Name) -> Self {
        val.0
    }
}

#[derive(Debug, Clone, Serialize)]
pub enum ToolType {
    Native,
    Mcp,
    Agent,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolCallPayload {
    tool_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cause: Option<String>,
}

impl ToolCallPayload {
    pub fn new(tool_name: String) -> Self {
        Self { tool_name, cause: None }
    }

    pub fn with_cause(mut self, cause: String) -> Self {
        self.cause = Some(cause);
        self
    }
}

#[derive(Debug, Clone)]
pub enum EventKind {
    Start,
    Ping,
    ToolCall(ToolCallPayload),
    Prompt(String),
    Error(String),
    Trace(Vec<u8>),
    Login(Identity),
    // Performance tracking events
    StartupPhase {
        phase: String,
        duration_ms: u64,
    },
    ServiceInit {
        service_name: String,
        duration_ms: u64,
    },
    ToolCallParsing {
        provider: String,
        tool_name: String,
        parsing_duration_ms: u64,
        validation_duration_ms: u64,
        arguments_size_bytes: usize,
    },
    ToolExecution {
        tool_name: String,
        tool_type: ToolType,
        execution_duration_ms: u64,
        queue_duration_ms: u64,
        output_size_bytes: usize,
        error_type: Option<String>,
    },
    LlmRequest {
        provider: String,
        model: String,
        request_prep_duration_ms: u64,
        network_duration_ms: u64,
        response_parse_duration_ms: u64,
        total_duration_ms: u64,
        input_tokens: u32,
        output_tokens: u32,
        is_streaming: bool,
    },
    ContextOperation {
        operation: String, // "clone", "transform", "append", "compact"
        duration_ms: u64,
        context_size_messages: usize,
        context_size_tokens: usize,
        memory_usage_bytes: usize,
    },
    Compaction {
        trigger_reason: String,
        duration_ms: u64,
        original_messages: usize,
        compacted_messages: usize,
        original_tokens: usize,
        compacted_tokens: usize,
        memory_saved_bytes: usize,
    },
    FileOperation {
        operation: String, // "read", "write", "search", "patch"
        file_path: String,
        file_size_bytes: usize,
        duration_ms: u64,
        lines_processed: usize,
    },
    MemorySnapshot {
        location: String,
        heap_usage_bytes: usize,
        clone_count: usize,
        operation: String,
    },
}

impl EventKind {
    pub fn name(&self) -> Name {
        match self {
            Self::Start => Name::from("start".to_string()),
            Self::Ping => Name::from("ping".to_string()),
            Self::Prompt(_) => Name::from("prompt".to_string()),
            Self::Error(_) => Name::from("error".to_string()),
            Self::ToolCall(_) => Name::from("tool_call".to_string()),
            Self::Trace(_) => Name::from("trace".to_string()),
            Self::Login(_) => Name::from("login".to_string()),
            Self::StartupPhase { .. } => Name::from("startup_phase".to_string()),
            Self::ServiceInit { .. } => Name::from("service_init".to_string()),
            Self::ToolCallParsing { .. } => Name::from("tool_call_parsing".to_string()),
            Self::ToolExecution { .. } => Name::from("tool_execution".to_string()),
            Self::LlmRequest { .. } => Name::from("llm_request".to_string()),
            Self::ContextOperation { .. } => Name::from("context_operation".to_string()),
            Self::Compaction { .. } => Name::from("compaction".to_string()),
            Self::FileOperation { .. } => Name::from("file_operation".to_string()),
            Self::MemorySnapshot { .. } => Name::from("memory_snapshot".to_string()),
        }
    }
    pub fn value(&self) -> String {
        match self {
            Self::Start => "".to_string(),
            Self::Ping => "".to_string(),
            Self::Prompt(content) => content.to_string(),
            Self::Error(content) => content.to_string(),
            Self::ToolCall(payload) => serde_json::to_string(&payload).unwrap_or_default(),
            Self::Trace(trace) => String::from_utf8_lossy(trace).to_string(),
            Self::Login(id) => id.login.to_owned(),
            Self::StartupPhase { phase, duration_ms } => {
                serde_json::to_string(&serde_json::json!({
                    "phase": phase,
                    "duration_ms": duration_ms
                }))
                .unwrap_or_default()
            }
            Self::ServiceInit { service_name, duration_ms } => {
                serde_json::to_string(&serde_json::json!({
                    "service_name": service_name,
                    "duration_ms": duration_ms
                }))
                .unwrap_or_default()
            }
            Self::ToolCallParsing {
                provider,
                tool_name,
                parsing_duration_ms,
                validation_duration_ms,
                arguments_size_bytes,
            } => serde_json::to_string(&serde_json::json!({
                "provider": provider,
                "tool_name": tool_name,
                "parsing_duration_ms": parsing_duration_ms,
                "validation_duration_ms": validation_duration_ms,
                "arguments_size_bytes": arguments_size_bytes
            }))
            .unwrap_or_default(),
            Self::ToolExecution {
                tool_name,
                tool_type,
                execution_duration_ms,
                queue_duration_ms,
                output_size_bytes,
                error_type,
            } => serde_json::to_string(&serde_json::json!({
                "tool_name": tool_name,
                "tool_type": tool_type,
                "execution_duration_ms": execution_duration_ms,
                "queue_duration_ms": queue_duration_ms,
                "output_size_bytes": output_size_bytes,
                "error_type": error_type
            }))
            .unwrap_or_default(),
            Self::LlmRequest {
                provider,
                model,
                request_prep_duration_ms,
                network_duration_ms,
                response_parse_duration_ms,
                total_duration_ms,
                input_tokens,
                output_tokens,
                is_streaming,
            } => serde_json::to_string(&serde_json::json!({
                "provider": provider,
                "model": model,
                "request_prep_duration_ms": request_prep_duration_ms,
                "network_duration_ms": network_duration_ms,
                "response_parse_duration_ms": response_parse_duration_ms,
                "total_duration_ms": total_duration_ms,
                "input_tokens": input_tokens,
                "output_tokens": output_tokens,
                "is_streaming": is_streaming
            }))
            .unwrap_or_default(),
            Self::ContextOperation {
                operation,
                duration_ms,
                context_size_messages,
                context_size_tokens,
                memory_usage_bytes,
            } => serde_json::to_string(&serde_json::json!({
                "operation": operation,
                "duration_ms": duration_ms,
                "context_size_messages": context_size_messages,
                "context_size_tokens": context_size_tokens,
                "memory_usage_bytes": memory_usage_bytes
            }))
            .unwrap_or_default(),
            Self::Compaction {
                trigger_reason,
                duration_ms,
                original_messages,
                compacted_messages,
                original_tokens,
                compacted_tokens,
                memory_saved_bytes,
            } => serde_json::to_string(&serde_json::json!({
                "trigger_reason": trigger_reason,
                "duration_ms": duration_ms,
                "original_messages": original_messages,
                "compacted_messages": compacted_messages,
                "original_tokens": original_tokens,
                "compacted_tokens": compacted_tokens,
                "memory_saved_bytes": memory_saved_bytes
            }))
            .unwrap_or_default(),
            Self::FileOperation {
                operation,
                file_path,
                file_size_bytes,
                duration_ms,
                lines_processed,
            } => serde_json::to_string(&serde_json::json!({
                "operation": operation,
                "file_path": file_path,
                "file_size_bytes": file_size_bytes,
                "duration_ms": duration_ms,
                "lines_processed": lines_processed
            }))
            .unwrap_or_default(),
            Self::MemorySnapshot { location, heap_usage_bytes, clone_count, operation } => {
                serde_json::to_string(&serde_json::json!({
                    "location": location,
                    "heap_usage_bytes": heap_usage_bytes,
                    "clone_count": clone_count,
                    "operation": operation
                }))
                .unwrap_or_default()
            }
        }
    }
}
