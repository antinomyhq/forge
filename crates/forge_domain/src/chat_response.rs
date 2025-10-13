use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{ToolCallFull, ToolName, ToolResult, Usage};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChatResponse {
    TaskMessage { content: ChatResponseContent },
    TaskReasoning { content: String },
    TaskComplete,
    ToolCallStart(ToolCallFull),
    ToolCallEnd(ToolResult),
    Usage(Usage),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InterruptionReason {
    MaxToolFailurePerTurnLimitReached {
        limit: u64,
        errors: HashMap<ToolName, usize>,
    },
    MaxRequestPerTurnLimitReached {
        limit: u64,
    },
}

#[derive(Clone, Serialize, Deserialize)]
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Category {
    Action,
    Info,
    Debug,
    Error,
    Completion,
}

#[derive(Clone, derive_setters::Setters, Debug, PartialEq, Serialize, Deserialize)]
#[setters(into, strip_option)]
pub struct TitleFormat {
    pub title: String,
    pub sub_title: Option<String>,
    pub category: Category,
    /// Optional timestamp for replay - when set, display logic uses this
    /// instead of current time
    #[serde(skip)]
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
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
            timestamp: None,
        }
    }

    /// Create a status for executing a tool
    pub fn action(message: impl Into<String>) -> Self {
        Self {
            title: message.into(),
            sub_title: None,
            category: Category::Action,
            timestamp: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            title: message.into(),
            sub_title: None,
            category: Category::Error,
            timestamp: None,
        }
    }

    pub fn debug(message: impl Into<String>) -> Self {
        Self {
            title: message.into(),
            sub_title: None,
            category: Category::Debug,
            timestamp: None,
        }
    }

    pub fn completion(message: impl Into<String>) -> Self {
        Self {
            title: message.into(),
            sub_title: None,
            category: Category::Completion,
            timestamp: None,
        }
    }
}
