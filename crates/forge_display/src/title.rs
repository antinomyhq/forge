use std::fmt::{self, Display, Formatter};

use derive_setters::Setters;

use crate::color::enhanced;

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
            Category::Action => enhanced::yellow("⏺"),
            Category::Info => enhanced::white("⏺"),
            Category::Debug => enhanced::cyan("⏺"),
            Category::Error => enhanced::red("⏺"),
            Category::Completion => enhanced::yellow("⏺"),
        };

        buf.push_str(format!("{icon} ").as_str());

        // Add timestamp if requested
        if with_timestamp {
            use chrono::Local;

            buf.push_str(
                enhanced::dimmed(&format!("[{}] ", Local::now().format("%H:%M:%S"))).as_str(),
            );
        }

        let title = match self.category {
            Category::Action => enhanced::white(&self.title),
            Category::Info => enhanced::white(&self.title),
            Category::Debug => enhanced::dimmed(&self.title),
            Category::Error => {
                enhanced::red(&format!("{} {}", enhanced::bold("ERROR:"), self.title))
            }
            Category::Completion => enhanced::bold(&enhanced::white(&self.title)),
        };

        buf.push_str(title.to_string().as_str());

        if let Some(ref sub_title) = self.sub_title {
            buf.push_str(&format!(" {}", enhanced::dimmed(sub_title)));
        }

        buf
    }
}

impl Display for TitleFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.render(true))
    }
}
