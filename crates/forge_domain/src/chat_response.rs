use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Local;
use tokio::sync::Notify;

use crate::{ToolCallFull, ToolName, ToolResult};

/// Acknowledgment handle for synchronization between sender and receiver.
///
/// When the receiver finishes processing an event that requires acknowledgment,
/// it should call `ack()` to notify the sender.
#[derive(Clone, Debug, Default)]
pub struct Ack(Option<Arc<Notify>>);

impl Ack {
    /// Creates a new acknowledgment handle.
    pub fn new() -> (Self, AckWaiter) {
        let notify = Arc::new(Notify::new());
        (Self(Some(notify.clone())), AckWaiter(notify))
    }

    /// Acknowledges that processing is complete.
    pub fn ack(&self) {
        if let Some(notify) = &self.0 {
            notify.notify_one();
        }
    }
}

/// Waiter for acknowledgment from the receiver.
pub struct AckWaiter(Arc<Notify>);

impl AckWaiter {
    /// Waits for acknowledgment from the receiver.
    pub async fn wait(self) {
        self.0.notified().await;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChatResponseContent {
    Title(TitleFormat),
    PlainText(String),
    Markdown(String),
}

impl From<ChatResponseContent> for ChatResponse {
    fn from(content: ChatResponseContent) -> Self {
        ChatResponse::TaskMessage { content }
    }
}

impl From<TitleFormat> for ChatResponse {
    fn from(title: TitleFormat) -> Self {
        ChatResponse::TaskMessage { content: ChatResponseContent::Title(title) }
    }
}

impl From<TitleFormat> for ChatResponseContent {
    fn from(title: TitleFormat) -> Self {
        ChatResponseContent::Title(title)
    }
}
impl ChatResponseContent {
    pub fn contains(&self, needle: &str) -> bool {
        self.as_str().contains(needle)
    }

    pub fn as_str(&self) -> &str {
        match self {
            ChatResponseContent::PlainText(text) | ChatResponseContent::Markdown(text) => text,
            ChatResponseContent::Title(_) => "",
        }
    }
}

/// Events that are emitted by the agent for external consumption. This includes
/// events for all internal state changes.
#[derive(Debug, Clone)]
pub enum ChatResponse {
    TaskMessage { content: ChatResponseContent },
    TaskReasoning { content: String },
    TaskComplete,
    /// Signals the start of a tool call.
    ///
    /// The receiver should call `ack.ack()` after flushing any pending output
    /// to synchronize with the sender before tool execution begins.
    ToolCallStart { tool_call: ToolCallFull, ack: Ack },
    ToolCallEnd(ToolResult),
    RetryAttempt { cause: Cause, duration: Duration },
    Interrupt { reason: InterruptionReason },
}

impl ChatResponse {
    /// Returns `true` if the response contains no meaningful content.
    ///
    /// A response is considered empty if it's a `TaskMessage` or
    /// `TaskReasoning` with empty string content. All other variants are
    /// considered non-empty.
    pub fn is_empty(&self) -> bool {
        match self {
            ChatResponse::TaskMessage { content } => match content {
                ChatResponseContent::Title(_) => false,
                ChatResponseContent::PlainText(content) => content.trim().is_empty(),
                ChatResponseContent::Markdown(content) => content.trim().is_empty(),
            },
            ChatResponse::TaskReasoning { content } => content.trim().is_empty(),
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum InterruptionReason {
    MaxToolFailurePerTurnLimitReached {
        limit: u64,
        errors: HashMap<ToolName, usize>,
    },
    MaxRequestPerTurnLimitReached {
        limit: u64,
    },
}

#[derive(Clone)]
pub struct Cause(String);

impl Cause {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl std::fmt::Debug for Cause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.as_str())
    }
}

impl From<&anyhow::Error> for Cause {
    fn from(value: &anyhow::Error) -> Self {
        Self(format!("{value:?}"))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Category {
    Action,
    Info,
    Debug,
    Error,
    Completion,
    Warning,
}

#[derive(Clone, derive_setters::Setters, Debug, PartialEq)]
#[setters(into, strip_option)]
pub struct TitleFormat {
    pub title: String,
    pub sub_title: Option<String>,
    pub category: Category,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

pub trait TitleExt {
    fn title_fmt(&self) -> TitleFormat;
}

impl<T> TitleExt for T
where
    T: Into<TitleFormat> + Clone,
{
    fn title_fmt(&self) -> TitleFormat {
        self.clone().into()
    }
}

impl TitleFormat {
    /// Create a status for executing a tool
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            title: message.into(),
            sub_title: None,
            category: Category::Info,
            timestamp: Local::now().into(),
        }
    }

    /// Create a status for executing a tool
    pub fn action(message: impl Into<String>) -> Self {
        Self {
            title: message.into(),
            sub_title: None,
            category: Category::Action,
            timestamp: Local::now().into(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            title: message.into(),
            sub_title: None,
            category: Category::Error,
            timestamp: Local::now().into(),
        }
    }

    pub fn debug(message: impl Into<String>) -> Self {
        Self {
            title: message.into(),
            sub_title: None,
            category: Category::Debug,
            timestamp: Local::now().into(),
        }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            title: message.into(),
            sub_title: None,
            category: Category::Warning,
            timestamp: Local::now().into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_title_format_with_timestamp() {
        let timestamp = DateTime::parse_from_rfc3339("2023-10-26T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let title = TitleFormat {
            title: "Test Action".to_string(),
            sub_title: Some("Subtitle".to_string()),
            category: Category::Action,
            timestamp,
        };

        assert_eq!(title.title, "Test Action");
        assert_eq!(title.sub_title, Some("Subtitle".to_string()));
        assert_eq!(title.category, Category::Action);
        assert_eq!(title.timestamp, timestamp);
    }
}
