use std::fmt::{self, Display, Formatter};

use colored::Colorize;
use derive_setters::Setters;

#[derive(Clone)]
pub enum Kind {
    Execute,
    Success,
    Failed,
    Retrying,
}

impl Kind {
    fn icon(&self) -> &'static str {
        match self {
            Kind::Execute => "⚙",
            Kind::Success => "✓",
            Kind::Failed => "✗",
            Kind::Retrying => "↻",
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Kind::Execute => "execute",
            Kind::Success => "success",
            Kind::Failed => "error",
            Kind::Retrying => "retrying",
        }
    }
}

#[derive(Clone, Setters)]
#[setters(into, strip_option)]
pub struct TitleFormat {
    pub kind: Kind,
    pub title: String,
    pub sub_title: Option<String>,
    pub error: Option<String>,
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
    pub fn execute(message: impl Into<String>) -> Self {
        Self {
            kind: Kind::Execute,
            title: message.into(),
            error: None,
            sub_title: Default::default(),
        }
    }

    /// Create a success status
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            kind: Kind::Success,
            title: message.into(),
            error: None,
            sub_title: Default::default(),
        }
    }

    /// Create a failure status
    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            kind: Kind::Failed,
            title: message.into(),
            error: None,
            sub_title: Default::default(),
        }
    }

    /// Create a retrying status
    pub fn retrying() -> Self {
        Self {
            kind: Kind::Retrying,
            title: "Retrying last message...".to_string(),
            error: None,
            sub_title: Default::default(),
        }
    }

    pub fn format(&self) -> String {
        let (icon, label, message) = match self.kind {
            Kind::Execute => (
                self.icon().cyan(),
                self.label().bold().cyan(),
                format!("{} ", self.title),
            ),
            Kind::Success => (
                self.icon().green(),
                self.label().bold().green(),
                self.title.to_string(),
            ),
            Kind::Failed => {
                let error_suffix = self
                    .error
                    .as_ref()
                    .map(|e| format!(" ({})", e))
                    .unwrap_or_default();
                (
                    self.icon().red(),
                    self.label().bold().red(),
                    format!("{}{}", self.title, error_suffix.red()),
                )
            }
            Kind::Retrying => (
                self.icon().yellow(),
                self.label().bold().yellow(),
                self.title.to_string(),
            ),
        };

        let timestamp = if cfg!(test) {
            // Use a fixed timestamp for tests to ensure snapshot consistency
            "10:00:00.000"
        } else {
            &chrono::Local::now().format("%H:%M:%S%.3f").to_string()
        };
        let mut result = format!("{} {} {} {}", timestamp.dimmed(), icon, label, message);

        if let Some(ref sub_title) = self.sub_title {
            result.push_str(&format!(" {}", sub_title).dimmed().to_string());
        }

        result
    }

    fn icon(&self) -> &'static str {
        self.kind.icon()
    }

    fn label(&self) -> &'static str {
        self.kind.label()
    }
}

impl Display for TitleFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format())
    }
}
