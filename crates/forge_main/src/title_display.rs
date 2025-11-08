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

    /// Builds the metadata string based on environment configuration
    fn build_metadata(&self) -> String {
        let env = self.env.as_ref();

        // Get the metadata format template
        let metadata_template = env
            .map(|e| e.title_metadata_format.as_str())
            .unwrap_or("{timestamp} {input}/{total} {cost} {cache_pct}");

        // Build individual components
        let timestamp = if env.map(|e| e.title_show_timestamp).unwrap_or(true) {
            format!("{}", Local::now().format("%H:%M:%S"))
        } else {
            String::new()
        };

        // Build usage/token info based on configuration and available data
        if let Some(ref usage) = self.inner.usage {
            let show_input = env.map(|e| e.title_show_input_tokens).unwrap_or(true);
            let show_total = env.map(|e| e.title_show_total_tokens).unwrap_or(true);
            let show_cost = env.map(|e| e.title_show_cost).unwrap_or(true);
            let show_cache = env.map(|e| e.title_show_cache_pct).unwrap_or(true);

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

            // Replace all placeholders in the template
            let mut result = metadata_template.to_string();

            // Replace timestamp
            if show_input && result.contains("{timestamp}") {
                result = result.replace("{timestamp}", &timestamp);
            } else {
                result = result.replace("{timestamp}", "");
            }

            // Replace token placeholders
            if show_input && result.contains("{input}") {
                result = result.replace("{input}", &input.to_string());
            } else {
                result = result.replace("{input}", "");
            }

            if result.contains("{output}") {
                result = result.replace("{output}", &output.to_string());
            } else {
                result = result.replace("{output}", "");
            }

            if show_total && result.contains("{total}") {
                result = result.replace("{total}", &total.to_string());
            } else {
                result = result.replace("{total}", "");
            }

            if result.contains("{cached}") {
                result = result.replace("{cached}", &cached.to_string());
            } else {
                result = result.replace("{cached}", "");
            }

            // Replace cache percentage
            if show_cache && result.contains("{cache_pct}") {
                result = result.replace("{cache_pct}", &format!("{}%", cache_pct));
            } else {
                result = result.replace("{cache_pct}", "");
            }

            // Replace cost
            if show_cost && !cost.is_empty() && result.contains("{cost}") {
                result = result.replace("{cost}", &cost);
            } else {
                result = result.replace("{cost}", "");
            }

            // Clean up extra spaces and slashes left from empty replacements
            result = result
                .split_whitespace()
                .filter(|s| !s.is_empty() && *s != "/")
                .collect::<Vec<_>>()
                .join(" ");

            result
        } else if let Some(limit) = self.inner.token_limit {
            // Fallback to token limit when no usage available
            let show_total = env.map(|e| e.title_show_total_tokens).unwrap_or(true);

            let mut result = metadata_template.to_string();

            // Replace with defaults for missing usage
            result = result.replace("{timestamp}", &timestamp);
            if show_total {
                result = result.replace("{total}", &limit.to_string());
                result = result.replace("{input}", "0");
            } else {
                result = result.replace("{total}", "");
                result = result.replace("{input}", "");
            }
            result = result.replace("{output}", "");
            result = result.replace("{cached}", "");
            result = result.replace("{cache_pct}", "");
            result = result.replace("{cost}", "");

            // Clean up extra spaces and slashes
            result = result
                .split_whitespace()
                .filter(|s| !s.is_empty() && *s != "/")
                .collect::<Vec<_>>()
                .join(" ");

            result
        } else {
            // No usage or token limit - just return timestamp if enabled
            if env.map(|e| e.title_show_timestamp).unwrap_or(true) {
                timestamp
            } else {
                String::new()
            }
        }
    }

    fn format_with_colors(&self) -> String {
        let icon = match self.inner.category {
            Category::Action => "⏺".yellow(),
            Category::Info => "⏺".white(),
            Category::Debug => "⏺".cyan(),
            Category::Error => "⏺".red(),
            Category::Completion => "⏺".yellow(),
        };

        let title = match self.inner.category {
            Category::Action => self.inner.title.white(),
            Category::Info => self.inner.title.white(),
            Category::Debug => self.inner.title.dimmed(),
            Category::Error => format!("{} {}", "ERROR:".bold(), self.inner.title).red(),
            Category::Completion => self.inner.title.white().bold(),
        };

        let subtitle = self
            .inner
            .sub_title
            .as_ref()
            .map(|s| s.dimmed().to_string())
            .unwrap_or_default();

        // Get format template from environment or use default
        let format_template = self
            .env
            .as_ref()
            .map(|e| e.title_format.as_str())
            .unwrap_or("[{metadata}] {title} {subtitle}");

        let metadata = self.build_metadata();

        // If metadata is empty, remove the entire [metadata] placeholder from template
        let format_template = if metadata.is_empty() {
            format_template
                .replace("[{metadata}]", "")
                .replace("{metadata}", "")
        } else {
            format_template.to_string()
        };

        let metadata_display = metadata.to_string().dimmed().to_string();

        // Replace placeholders
        let result = format_template
            .replace("{icon}", &icon.to_string())
            .replace("{metadata}", &metadata_display)
            .replace("{title}", &title.to_string())
            .replace("{subtitle}", &subtitle);

        // Prepend icon if not in template
        if !format_template.contains("{icon}") {
            format!("{} {}", icon, result.trim())
        } else {
            result.trim().to_string()
        }
    }

    fn format_plain(&self) -> String {
        let icon = "⏺";
        let title = &self.inner.title;
        let subtitle = self.inner.sub_title.as_deref().unwrap_or("");

        // Get format template from environment or use default
        let format_template = self
            .env
            .as_ref()
            .map(|e| e.title_format.as_str())
            .unwrap_or("[{metadata}] {title} {subtitle}");

        let metadata = self.build_metadata();

        // If metadata is empty, remove the entire [metadata] placeholder from template
        let format_template = if metadata.is_empty() {
            format_template
                .replace("[{metadata}]", "")
                .replace("{metadata}", "")
        } else {
            format_template.to_string()
        };

        let metadata_display = metadata.to_string();

        // Replace placeholders
        let result = format_template
            .replace("{icon}", icon)
            .replace("{metadata}", &metadata_display)
            .replace("{title}", title)
            .replace("{subtitle}", subtitle);

        // Prepend icon if not in template
        if !format_template.contains("{icon}") {
            format!("{} {}", icon, result.trim())
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
