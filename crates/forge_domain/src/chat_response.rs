use std::collections::HashMap;
use std::time::Duration;

use crate::{ToolCallFull, ToolName, ToolResult, Usage};

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
}

#[derive(Clone, derive_setters::Setters, Debug, PartialEq)]
#[setters(into, strip_option)]
pub struct TitleFormat {
    pub title: String,
    pub sub_title: Option<String>,
    pub category: Category,
    /// Optional timestamp for replay - when set, display logic uses this
    /// instead of current time
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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use chrono::{DateTime, Utc};

    #[test]
    fn test_title_format_with_timestamp() {
        let timestamp = DateTime::parse_from_rfc3339("2023-10-26T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        
        let title = TitleFormat {
            title: "Test Action".to_string(),
            sub_title: Some("Subtitle".to_string()),
            category: Category::Action,
            timestamp: Some(timestamp),
        };

        assert_eq!(title.title, "Test Action");
        assert_eq!(title.sub_title, Some("Subtitle".to_string()));
        assert_eq!(title.category, Category::Action);
        assert_eq!(title.timestamp, Some(timestamp));
    }

    #[test]
    fn test_title_format_without_timestamp() {
        let title = TitleFormat::info("Test Info");
        
        assert_eq!(title.title, "Test Info");
        assert_eq!(title.sub_title, None);
        assert_eq!(title.category, Category::Info);
        assert_eq!(title.timestamp, None);
    }

    #[test]
    fn test_title_format_constructors_with_none_timestamp() {
        let info = TitleFormat::info("Info message");
        let action = TitleFormat::action("Action message");
        let error = TitleFormat::error("Error message");
        let debug = TitleFormat::debug("Debug message");
        let completion = TitleFormat::completion("Completion message");

        // All constructors should initialize timestamp as None
        assert_eq!(info.timestamp, None);
        assert_eq!(action.timestamp, None);
        assert_eq!(error.timestamp, None);
        assert_eq!(debug.timestamp, None);
        assert_eq!(completion.timestamp, None);

        // Verify other fields
        assert_eq!(info.category, Category::Info);
        assert_eq!(action.category, Category::Action);
        assert_eq!(error.category, Category::Error);
        assert_eq!(debug.category, Category::Debug);
        assert_eq!(completion.category, Category::Completion);
    }

    #[test]
    fn test_title_format_with_timestamp_setter() {
        let timestamp = DateTime::parse_from_rfc3339("2023-10-26T14:45:30Z")
            .unwrap()
            .with_timezone(&Utc);
        
        let title = TitleFormat::info("Test")
            .timestamp(timestamp)
            .sub_title("With timestamp");

        assert_eq!(title.timestamp, Some(timestamp));
        assert_eq!(title.sub_title, Some("With timestamp".to_string()));
    }
}