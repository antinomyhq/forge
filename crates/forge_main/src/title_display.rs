use std::fmt;

use chrono::Local;
use colored::Colorize;
use forge_domain::{Category, Environment, TitleFormat, Usage};

/// Implementation of Display for TitleFormat in the presentation layer
pub struct TitleDisplay {
    inner: TitleFormat,
    with_colors: bool,
    env: Option<Environment>,
}

impl TitleDisplay {
    pub fn new(title: TitleFormat) -> Self {
        Self {
            inner: title,
            with_colors: true,
            env: None,
        }
    }

    pub fn with_colors(mut self, with_colors: bool) -> Self {
        self.with_colors = with_colors;
        self
    }

    pub fn with_env(mut self, env: Environment) -> Self {
        self.env = Some(env);
        self
    }

    /// Format the title with optional usage and token limit data
    pub fn format_with_data(&self, usage: Option<&Usage>, token_limit: Option<usize>) -> String {
        // Get format template from environment or use default
        let format_template = self
            .env
            .as_ref()
            .map(|e| e.title_format.as_str())
            .unwrap_or("[{timestamp} {input}/{output} {cost} {cache_pct}] {title} {subtitle}");

        let result = self.apply_format(format_template, self.with_colors, usage, token_limit);

        // Prepend icon if not in template
        if !format_template.contains("{icon}") {
            if self.with_colors {
                let icon = match self.inner.category {
                    Category::Action => "⏺".yellow(),
                    Category::Info => "⏺".white(),
                    Category::Debug => "⏺".cyan(),
                    Category::Error => "⏺".red(),
                    Category::Completion => "⏺".yellow(),
                };
                format!("{} {}", icon, result.trim()).trim().to_string()
            } else {
                format!("⏺ {}", result.trim())
            }
        } else {
            result.trim().to_string()
        }
    }

    /// Replaces all placeholders in the format template with actual values
    fn apply_format(
        &self,
        template: &str,
        with_colors: bool,
        usage: Option<&Usage>,
        token_limit: Option<usize>,
    ) -> String {
        let mut result = template.to_string();

        // Replace timestamp
        let timestamp = {
            let local_time: chrono::DateTime<Local> = self.inner.timestamp.into();
            format!("{}", local_time.format("%H:%M:%S"))
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
        if let Some(usage) = usage {
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
        } else if let Some(limit) = token_limit {
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
        // First, clean up spaces within brackets
        result = result
            .replace("[ ", "[")
            .replace(" ]", "]")
            .replace("[/", "[")
            .replace("/]", "]");

        // Then normalize multiple spaces and slashes to single space
        let result = result
            .split_whitespace()
            .filter(|s| !s.is_empty() && *s != "/")
            .collect::<Vec<_>>()
            .join(" ");

        // Finally, clean up empty brackets completely
        let result = result.replace("[]", "");

        result
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

        let local_time: chrono::DateTime<Local> = self.inner.timestamp.into();
        let timestamp_str = format!("[{}] ", local_time.format("%H:%M:%S"));
        buf.push_str(timestamp_str.dimmed().to_string().as_str());

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

        let local_time: chrono::DateTime<Local> = self.inner.timestamp.into();
        let timestamp_str = format!("[{}] ", local_time.format("%H:%M:%S"));
        buf.push_str(&timestamp_str);

        buf.push_str(&self.inner.title);

        if let Some(ref sub_title) = self.inner.sub_title {
            buf.push(' ');
            buf.push_str(sub_title);
        }

        buf
    }
}

impl fmt::Display for TitleDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Check if we have an environment with a custom format
        if self.env.is_some() {
            write!(f, "{}", self.format_with_data(None, None))
        } else if self.with_colors {
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
    fn display_with_env(self, env: Environment) -> TitleDisplay;
}

impl TitleDisplayExt for TitleFormat {
    fn display(self) -> TitleDisplay {
        TitleDisplay::new(self)
    }

    fn display_with_colors(self, with_colors: bool) -> TitleDisplay {
        TitleDisplay::new(self).with_colors(with_colors)
    }

    fn display_with_env(self, env: Environment) -> TitleDisplay {
        TitleDisplay::new(self).with_env(env)
    }
}

#[cfg(test)]
mod tests {
    use fake::Fake;
    use forge_domain::{Category, TokenCount};

    use super::*;

    #[test]
    fn test_title_display_without_usage_cleans_empty_brackets() {
        let title = TitleFormat {
            title: "Test Title".to_string(),
            sub_title: None,
            category: Category::Info,
            timestamp: chrono::Utc::now(),
        };

        let env = Environment {
            title_format: "[{timestamp} {input}/{total} {cost}] {title}".to_string(),
            ..fake::Faker.fake()
        };

        let display = title.display_with_env(env).with_colors(false);

        let result = display.format_with_data(None, None);

        // Should not have empty brackets or trailing spaces
        assert!(!result.contains("[]"));
        assert!(!result.contains(" ]"));
        assert!(result.contains("Test Title"));
    }

    #[test]
    fn test_title_display_with_usage_shows_all_fields() {
        let title = TitleFormat {
            title: "Test Title".to_string(),
            sub_title: None,
            category: Category::Info,
            timestamp: chrono::Utc::now(),
        };

        let usage = Usage {
            prompt_tokens: TokenCount::Actual(100),
            completion_tokens: TokenCount::Actual(50),
            total_tokens: TokenCount::Actual(150),
            cached_tokens: TokenCount::Actual(20),
            cost: Some(0.05),
        };

        let env = Environment {
            title_format: "[{input}/{total} {cost}] {title}".to_string(),
            ..fake::Faker.fake()
        };

        let display = title.display_with_env(env).with_colors(false);

        let result = display.format_with_data(Some(&usage), None);

        assert!(result.contains("100/150"));
        assert!(result.contains("$0.0500"));
        assert!(result.contains("Test Title"));
        assert!(!result.contains(" ]")); // No trailing space before bracket
    }

    #[test]
    fn test_title_display_with_subtitle() {
        let title = TitleFormat {
            title: "Test Title".to_string(),
            sub_title: Some("Subtitle".to_string()),
            category: Category::Debug,
            timestamp: chrono::Utc::now(),
        };

        let env = Environment {
            title_format: "{title} {subtitle}".to_string(),
            ..fake::Faker.fake()
        };

        let display = title.display_with_env(env).with_colors(false);

        let result = display.format_with_data(None, None);

        assert!(result.contains("Test Title"));
        assert!(result.contains("Subtitle"));
    }
}
