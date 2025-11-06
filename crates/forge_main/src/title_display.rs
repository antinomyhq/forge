use std::fmt;

use chrono::Local;
use colored::Colorize;
use forge_domain::{Category, TitleFormat};

/// Implementation of Display for TitleFormat in the presentation layer
pub struct TitleDisplay {
    inner: TitleFormat,
    with_timestamp: bool,
    with_colors: bool,
}

impl TitleDisplay {
    pub fn new(title: TitleFormat) -> Self {
        Self { inner: title, with_timestamp: true, with_colors: true }
    }

    pub fn with_timestamp(mut self, with_timestamp: bool) -> Self {
        self.with_timestamp = with_timestamp;
        self
    }

    pub fn with_colors(mut self, with_colors: bool) -> Self {
        self.with_colors = with_colors;
        self
    }

    fn format_with_colors(&self) -> String {
        let mut buf = String::new();

        let icon = match self.inner.category {
            Category::Action => "⏺".yellow(),
            Category::Info => "⏺".white(),
            Category::Debug => "⏺".cyan(),
            Category::Error => "⏺".red(),
            Category::Completion => "⏺".yellow(),
        };

        buf.push_str(format!("{icon} ").as_str());

        // Add timestamp if requested
        if self.with_timestamp {
            let timestamp = format!("{}", Local::now().format("%H:%M:%S"));

            // Add usage information inline with timestamp if available
            if let Some(ref usage) = self.inner.usage {
                let input = *usage.prompt_tokens;
                let _output = *usage.completion_tokens;
                let total = *usage.total_tokens;

                // Calculate cache percentage
                let cached = *usage.cached_tokens;
                let cache_pct = if total > 0 {
                    (cached as f64 / total as f64 * 100.0) as u64
                } else {
                    0
                };

                let cost_str = usage
                    .cost
                    .map(|c| format!(" ${:.4}", c))
                    .unwrap_or_default();

                buf.push_str(
                    format!(
                        "[{} {}/{}{} {}%] ",
                        timestamp, input, total, cost_str, cache_pct
                    )
                    .dimmed()
                    .to_string()
                    .as_str(),
                );
            } else if let Some(limit) = self.inner.token_limit {
                // Show token limit as fallback when usage is not available
                buf.push_str(
                    format!("[{} 0/{}] ", timestamp, limit)
                        .dimmed()
                        .to_string()
                        .as_str(),
                );
            } else {
                buf.push_str(format!("[{}] ", timestamp).dimmed().to_string().as_str());
            }
        }

        let title = match self.inner.category {
            Category::Action => self.inner.title.white(),
            Category::Info => self.inner.title.white(),
            Category::Debug => self.inner.title.dimmed(),
            Category::Error => format!("{} {}", "ERROR:".bold(), self.inner.title).red(),
            Category::Completion => self.inner.title.white().bold(),
        };

        buf.push_str(title.to_string().as_str());

        if let Some(ref sub_title) = self.inner.sub_title {
            buf.push_str(&format!(" {}", sub_title.dimmed()).to_string());
        }

        buf
    }

    fn format_plain(&self) -> String {
        let mut buf = String::new();

        buf.push_str("⏺ ");

        // Add timestamp if requested
        if self.with_timestamp {
            let timestamp = format!("{}", Local::now().format("%H:%M:%S"));

            // Add usage information inline with timestamp if available
            if let Some(ref usage) = self.inner.usage {
                let input = *usage.prompt_tokens;
                let _output = *usage.completion_tokens;
                let total = *usage.total_tokens;

                // Calculate cache percentage
                let cached = *usage.cached_tokens;
                let cache_pct = if total > 0 {
                    (cached as f64 / total as f64 * 100.0) as u64
                } else {
                    0
                };

                let cost_str = usage
                    .cost
                    .map(|c| format!(" ${:.4}", c))
                    .unwrap_or_default();

                buf.push_str(&format!(
                    "[{} {}/{}{} {}%] ",
                    timestamp, input, total, cost_str, cache_pct
                ));
            } else if let Some(limit) = self.inner.token_limit {
                // Show token limit as fallback when usage is not available
                buf.push_str(&format!("[{} 0/{}] ", timestamp, limit));
            } else {
                buf.push_str(format!("[{}] ", timestamp).as_str());
            }
        }

        buf.push_str(&self.inner.title);

        if let Some(ref sub_title) = self.inner.sub_title {
            buf.push_str(&format!(" {sub_title}"));
        }

        buf
    }
}

impl fmt::Display for TitleDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.with_colors {
            write!(f, "{}", self.format_with_colors())
        } else {
            write!(f, "{}", self.format_plain())
        }
    }
}

/// Extension trait to easily convert TitleFormat to displayable form
pub trait TitleDisplayExt {
    fn display(self) -> TitleDisplay;
    fn display_with_colors(self, with_colors: bool) -> TitleDisplay;
    fn display_with_timestamp(self, with_timestamp: bool) -> TitleDisplay;
}

impl TitleDisplayExt for TitleFormat {
    fn display(self) -> TitleDisplay {
        TitleDisplay::new(self)
    }

    fn display_with_colors(self, with_colors: bool) -> TitleDisplay {
        TitleDisplay::new(self).with_colors(with_colors)
    }

    fn display_with_timestamp(self, with_timestamp: bool) -> TitleDisplay {
        TitleDisplay::new(self).with_timestamp(with_timestamp)
    }
}
