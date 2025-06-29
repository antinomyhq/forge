use std::fmt::{self, Display, Formatter};

use colored::Colorize;
use derive_setters::Setters;

#[derive(Clone, Debug, PartialEq)]
pub enum Category {
    Action,
    Info,
    Debug,
    Error,
    Completion,
}

#[derive(Clone, Setters, Debug, PartialEq)]
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

    fn format(&self, with_timestamp: bool) -> String {
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
}

impl Display for TitleFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.render(true))
    }
}
