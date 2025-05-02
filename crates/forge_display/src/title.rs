use std::fmt::{self, Display, Formatter};

use colored::Colorize;
use derive_setters::Setters;

#[derive(Clone, Setters)]
#[setters(into, strip_option)]
pub struct TitleFormat {
    pub title: String,
    pub sub_title: Option<String>,
    pub error: Option<String>,
    pub is_user_action: bool,
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
    pub fn new(tag: impl Into<String>) -> Self {
        Self {
            title: tag.into(),
            error: None,
            sub_title: Default::default(),
            is_user_action: false,
        }
    }

    /// Create a status for executing a tool
    pub fn action(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            error: None,
            sub_title: Default::default(),
            is_user_action: true,
        }
    }

    pub fn format(&self) -> String {
        let mut buf = String::new();

        if self.error.is_some() {
            buf.push_str(format!("{} ", "⏺".red()).as_str());
        } else if self.is_user_action {
            buf.push_str(format!("{} ", "⏺".yellow()).as_str());
        } else {
            buf.push_str(format!("{} ", "⏺".cyan()).as_str());
        }

        // Add timestamp at the beginning if this is not a user action
        #[cfg(not(test))]
        {
            use chrono::Local;

            buf.push_str(
                format!("[{}] ", Local::now().format("%H:%M:%S.%3f"))
                    .dimmed()
                    .to_string()
                    .as_str(),
            );
        }

        if self.error.is_some() {
            buf.push_str(self.title.red().bold().to_string().as_str())
        } else if self.is_user_action {
            buf.push_str(self.title.as_str())
        } else {
            buf.push_str(self.title.dimmed().to_string().as_str())
        };

        if let Some(ref sub_title) = self.sub_title {
            buf.push_str(&format!(" {}", sub_title.dimmed()).to_string());
        }

        if let Some(ref error) = self.error {
            buf.push_str(&format!(" {error}").to_string());
        }

        buf
    }
}

impl Display for TitleFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format())
    }
}
