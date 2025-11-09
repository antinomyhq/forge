use std::fmt;

use chrono::Local;
use colored::Colorize;
use forge_domain::{Category, Environment, TitleFormat};

/// Implementation of Display for TitleFormat in the presentation layer
pub struct TitleDisplay {
    inner: TitleFormat,
    with_timestamp: bool,
    with_colors: bool,
    env: Option<Environment>,
}

impl TitleDisplay {
    pub fn new(title: TitleFormat) -> Self {
        Self {
            inner: title,
            with_timestamp: true,
            with_colors: true,
            env: None,
        }
    }

    pub fn with_timestamp(mut self, with_timestamp: bool) -> Self {
        self.with_timestamp = with_timestamp;
        self
    }

    pub fn with_colors(mut self, with_colors: bool) -> Self {
        self.with_colors = with_colors;
        self
    }

    pub fn with_env(mut self, env: Environment) -> Self {
        self.env = Some(env);
        self
    }

    /// Replaces all placeholders in the format template with actual values
    fn apply_format(&self, template: &str, with_colors: bool) -> String {
        let mut result = template.to_string();

        // Replace timestamp
        let timestamp = if self.with_timestamp {
            format!("{}", Local::now().format("%H:%M:%S"))
        } else {
            String::new()
        };
        result = result.replace("{timestamp}", &timestamp);

        // Replace title and subtitle
        let title = if with_colors {
            match self.inner.category {
                Category::Action => self.inner.title.white().to_string(),
                Category::Info => self.inner.title.white().to_string(),
                Category::Debug => self.inner.title.dimmed().to_string(),
                Category::Error => format!("{} {}", "ERROR:".bold(), self.inner.title)
                    .red()
                    .to_string(),
                Category::Completion => self.inner.title.white().bold().to_string(),
            }
        } else {
            self.inner.title.clone()
        };

        let subtitle = self
            .inner
            .sub_title
            .as_ref()
            .map(|s| {
                if with_colors {
                    s.dimmed().to_string()
                } else {
                    s.clone()
                }
            })
            .unwrap_or_default();

        result = result.replace("{title}", &title);
        result = result.replace("{subtitle}", &subtitle);

        // Replace icon
        let icon = if with_colors {
            match self.inner.category {
                Category::Action => "⏺".yellow().to_string(),
                Category::Info => "⏺".white().to_string(),
                Category::Debug => "⏺".cyan().to_string(),
                Category::Error => "⏺".red().to_string(),
                Category::Completion => "⏺".yellow().to_string(),
            }
        } else {
            "⏺".to_string()
        };
        result = result.replace("{icon}", &icon);

        // Replace usage/token info based on available data
        if let Some(ref usage) = self.inner.usage {
            let input = *usage.prompt_tokens;
            let output = *usage.completion_tokens;
            let total = *usage.total_tokens;
            let cached = *usage.cached_tokens;
            let cache_pct = if total > 0 {
                (cached as f64 / total as f64 * 100.0) as u64
            } else {
                0
            };
            let cost = usage.cost.map(|c| format!("${:.4}", c)).unwrap_or_default();

            result = result.replace("{input}", &input.to_string());
            result = result.replace("{output}", &output.to_string());
            result = result.replace("{total}", &total.to_string());
            result = result.replace("{cached}", &cached.to_string());
            result = result.replace("{cache_pct}", &format!("{}%", cache_pct));
            result = result.replace("{cost}", &cost);
        } else if let Some(limit) = self.inner.token_limit {
            // Fallback to token limit when no usage available
            result = result.replace("{total}", &limit.to_string());
            result = result.replace("{input}", "0");
            result = result.replace("{output}", "");
            result = result.replace("{cached}", "");
            result = result.replace("{cache_pct}", "");
            result = result.replace("{cost}", "");
        } else {
            // No usage or token limit - replace with empty strings
            result = result.replace("{input}", "");
            result = result.replace("{output}", "");
            result = result.replace("{total}", "");
            result = result.replace("{cached}", "");
            result = result.replace("{cache_pct}", "");
            result = result.replace("{cost}", "");
        }

        // Clean up extra spaces, brackets, and slashes left from empty replacements
        result = result
            .replace("[]", "")
            .replace("[ ]", "")
            .replace("/}", "")
            .replace("{/", "");

        result = result
            .split_whitespace()
            .filter(|s| !s.is_empty() && *s != "/")
            .collect::<Vec<_>>()
            .join(" ");

        result
    }

    fn format_with_colors(&self) -> String {
        // Get format template from environment or use default
        let format_template = self
            .env
            .as_ref()
            .map(|e| e.title_format.as_str())
            .unwrap_or("[{timestamp} {input}/{total} {cost} {cache_pct}] {title} {subtitle}");

        let result = self.apply_format(format_template, true);

        // Prepend icon if not in template
        if !format_template.contains("{icon}") {
            let icon = match self.inner.category {
                Category::Action => "⏺".yellow(),
                Category::Info => "⏺".white(),
                Category::Debug => "⏺".cyan(),
                Category::Error => "⏺".red(),
                Category::Completion => "⏺".yellow(),
            };
            format!("{} {}", icon, result.trim())
        } else {
            result.trim().to_string()
        }
    }

    fn format_plain(&self) -> String {
        // Get format template from environment or use default
        let format_template = self
            .env
            .as_ref()
            .map(|e| e.title_format.as_str())
            .unwrap_or("[{timestamp} {input}/{total} {cost} {cache_pct}] {title} {subtitle}");

        let result = self.apply_format(format_template, false);

        // Prepend icon if not in template
        if !format_template.contains("{icon}") {
            format!("⏺ {}", result.trim())
        } else {
            result.trim().to_string()
        }
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
    fn display_with_env(self, env: Environment) -> TitleDisplay;
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

    fn display_with_env(self, env: Environment) -> TitleDisplay {
        TitleDisplay::new(self).with_env(env)
    }
}
