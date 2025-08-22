use std::time::Duration;

use crate::{Metrics, ToolCallFull, ToolResult, Usage};

#[derive(Debug, Clone, PartialEq)]
pub enum ChatResponseContent {
    Title(String),
    PlainText(String),
    Markdown(String),
}

impl From<ChatResponseContent> for ChatResponse {
    fn from(content: ChatResponseContent) -> Self {
        ChatResponse::TaskMessage { content }
    }
}
impl ChatResponseContent {
    pub fn contains(&self, needle: &str) -> bool {
        self.to_string().contains(needle)
    }

    pub fn as_str(&self) -> &str {
        match self {
            ChatResponseContent::PlainText(text) | ChatResponseContent::Markdown(text) => text,
            ChatResponseContent::Title(_) => {
                // For titles, we can't return a reference to the formatted string
                // since it's computed on demand. Tests should use to_string() instead.
                panic!("as_str() not supported for Title format, use to_string() instead")
            }
        }
    }

    pub fn render(&self, with_timestamp: bool) -> String {
        match self {
            ChatResponseContent::Title(title) => {
                if with_timestamp {
                    title.clone()
                } else {
                    // Remove timestamp pattern from title if present, accounting for ANSI codes
                    let re = regex::Regex::new(r"(\x1b\[[0-9;]*m)*⏺(\x1b\[[0-9;]*m)* (\x1b\[[0-9;]*m)*\[(\x1b\[[0-9;]*m)*\d{2}:\d{2}:\d{2}(\x1b\[[0-9;]*m)*\] (\x1b\[[0-9;]*m)*").unwrap();
                    
                    re.replace(title, "${1}⏺${2} ").to_string()
                }
            }
            ChatResponseContent::PlainText(text) => text.clone(),
            ChatResponseContent::Markdown(text) => text.clone(),
        }
    }
}

impl std::fmt::Display for ChatResponseContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatResponseContent::Title(title) => write!(f, "{title}"),
            ChatResponseContent::PlainText(text) => write!(f, "{text}"),
            ChatResponseContent::Markdown(text) => write!(f, "{text}"),
        }
    }
}

/// Events that are emitted by the agent for external consumption. This includes
/// events for all internal state changes.
#[derive(Debug, Clone)]
pub enum ChatResponse {
    TaskMessage { content: ChatResponseContent },
    TaskReasoning { content: String },
    TaskComplete { metrics: Metrics },
    ToolCallStart(ToolCallFull),
    ToolCallEnd(ToolResult),
    Usage(Usage),
    RetryAttempt { cause: Cause, duration: Duration },
    Interrupt { reason: InterruptionReason },
}

#[derive(Debug, Clone)]
pub enum InterruptionReason {
    MaxToolFailurePerTurnLimitReached { limit: u64 },
    MaxRequestPerTurnLimitReached { limit: u64 },
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
        }
    }

    /// Create a status for executing a tool
    pub fn action(message: impl Into<String>) -> Self {
        Self {
            title: message.into(),
            sub_title: None,
            category: Category::Action,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            title: message.into(),
            sub_title: None,
            category: Category::Error,
        }
    }

    pub fn debug(message: impl Into<String>) -> Self {
        Self {
            title: message.into(),
            sub_title: None,
            category: Category::Debug,
        }
    }

    pub fn completion(message: impl Into<String>) -> Self {
        Self {
            title: message.into(),
            sub_title: None,
            category: Category::Completion,
        }
    }

    pub fn render(&self, with_timestamp: bool) -> String {
        self.format(with_timestamp)
    }

    pub fn render_with_colors(&self, with_timestamp: bool) -> String {
        use colored::Colorize;

        let mut buf = String::new();

        let icon = match self.category {
            Category::Action => "⏺".yellow(),
            Category::Info => "⏺".white(),
            Category::Debug => "⏺".cyan(),
            Category::Error => "⏺".red(),
            Category::Completion => "⏺".yellow(),
        };

        buf.push_str(format!("{icon} ").as_str());

        // Add timestamp if requested
        if with_timestamp {
            use chrono::Local;

            buf.push_str(
                format!("[{}] ", Local::now().format("%H:%M:%S"))
                    .dimmed()
                    .to_string()
                    .as_str(),
            );
        }

        let title = match self.category {
            Category::Action => self.title.white(),
            Category::Info => self.title.white(),
            Category::Debug => self.title.dimmed(),
            Category::Error => format!("{} {}", "ERROR:".bold(), self.title).red(),
            Category::Completion => self.title.white().bold(),
        };

        buf.push_str(title.to_string().as_str());

        if let Some(ref sub_title) = self.sub_title {
            buf.push_str(&format!(" {}", sub_title.dimmed()).to_string());
        }

        buf
    }

    fn format(&self, with_timestamp: bool) -> String {
        let mut buf = String::new();

        let icon = "⏺";
        buf.push_str(format!("{icon} ").as_str());

        // Add timestamp if requested
        if with_timestamp {
            use chrono::Local;
            buf.push_str(format!("[{}] ", Local::now().format("%H:%M:%S")).as_str());
        }

        buf.push_str(&self.title);

        if let Some(ref sub_title) = self.sub_title {
            buf.push_str(&format!(" {}", sub_title));
        }

        buf
    }
}

impl std::fmt::Display for TitleFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.render_with_colors(true))
    }
}
