use std::collections::HashMap;
use std::fmt;

use chrono::Local;
use colored::Colorize;
use forge_domain::{Category, Environment, TitleFormat, Usage};
use forge_template::MustacheTemplateEngine;

/// Implementation of Display for TitleFormat in the presentation layer
pub struct TitleDisplay {
    inner: TitleFormat,
    with_colors: bool,
    env: Option<Environment>,
}

impl TitleDisplay {
    pub fn new(title: TitleFormat) -> Self {
        Self { inner: title, with_colors: true, env: None }
    }

    pub fn with_colors(mut self, with_colors: bool) -> Self {
        self.with_colors = with_colors;
        self
    }

    pub fn with_env(mut self, env: Environment) -> Self {
        self.env = Some(env);
        self
    }

    /// Get icon string based on category with appropriate coloring
    fn get_icon(&self, colored: bool) -> String {
        let (icon, color_fn): (&str, fn(&str) -> colored::ColoredString) = match self.inner.category
        {
            Category::Action => ("⏺", |s| s.yellow()),
            Category::Info => ("⏺", |s| s.white()),
            Category::Debug => ("⏺", |s| s.cyan()),
            Category::Error => ("❌", |s| s.red()),
            Category::Completion => ("⏺", |s| s.yellow()),
            Category::Warning => ("⚠️", |s| s.bright_yellow()),
        };

        if colored {
            color_fn(icon).to_string()
        } else {
            icon.to_string()
        }
    }

    /// Get formatted timestamp
    fn get_timestamp(&self) -> String {
        let local_time: chrono::DateTime<Local> = self.inner.timestamp.into();
        format!("{}", local_time.format("%H:%M:%S"))
    }

    /// Build template data map with common fields
    fn build_template_data(
        &self,
        usage: Option<&Usage>,
        token_limit: Option<usize>,
    ) -> HashMap<String, String> {
        let mut data = HashMap::new();

        // Add level
        let level = self.inner.category.to_string().to_ascii_lowercase();
        data.insert("level".to_string(), level);

        // Add timestamp
        data.insert("timestamp".to_string(), self.get_timestamp());

        // Add title and subtitle
        data.insert("title".to_string(), self.inner.title.clone());
        data.insert(
            "subtitle".to_string(),
            self.inner.sub_title.as_ref().cloned().unwrap_or_default(),
        );

        // Add icon
        data.insert("icon".to_string(), self.get_icon(self.with_colors));

        // Add usage/token info based on available data
        self.add_usage_data(&mut data, usage, token_limit);

        data
    }

    /// Add usage data to the template data map
    fn add_usage_data(
        &self,
        data: &mut HashMap<String, String>,
        usage: Option<&Usage>,
        token_limit: Option<usize>,
    ) {
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

            data.insert("input".to_string(), input.to_string());
            data.insert("output".to_string(), output.to_string());
            data.insert("total".to_string(), total.to_string());
            data.insert("cached".to_string(), cached.to_string());
            data.insert("cache_pct".to_string(), format!("{}%", cache_pct));
            data.insert("cost".to_string(), cost);
            data.insert("has_usage".to_string(), "true".to_string());
        } else {
            // Set empty strings for all usage fields
            data.insert("input".to_string(), String::new());
            data.insert("output".to_string(), String::new());
            data.insert("cached".to_string(), String::new());
            data.insert("cache_pct".to_string(), String::new());
            data.insert("cost".to_string(), String::new());
            data.insert("has_usage".to_string(), String::new());

            // Set total from token_limit if available
            if let Some(limit) = token_limit {
                data.insert("total".to_string(), limit.to_string());
            } else {
                data.insert("total".to_string(), String::new());
            }
        }
    }

    /// Format the title with optional usage and token limit data using Mustache
    /// templates
    pub fn format_with_data(&self, usage: Option<&Usage>, token_limit: Option<usize>) -> String {
        // Get format template from environment or use default
        let format_template = self.env.as_ref().map(|e| e.title_format.as_str()).unwrap_or(
            r#"{{#if (is_not_empty has_usage)}}{{dimmed "["}}{{white timestamp}} {{white input}}{{#if (is_not_empty output)}}/{{white output}}{{/if}}{{#if (is_not_empty cost)}} {{white cost}}{{/if}}{{#if (is_not_empty cache_pct)}} {{white cache_pct}}{{/if}}{{dimmed "]"}} {{/if}}{{white title}}{{#if (is_not_empty subtitle)}} {{dimmed subtitle}}{{/if}}"#,
        );

        // Build data map for template
        let data = self.build_template_data(usage, token_limit);

        // Create template engine and render
        let mut engine = MustacheTemplateEngine::new(self.with_colors);
        engine.render(format_template, &data).unwrap_or_else(|_| {
            format!(
                "⏺ {} {}",
                self.inner.title,
                self.inner.sub_title.as_deref().unwrap_or("")
            )
        })
    }

    fn format_with_colors(&self) -> String {
        let mut buf = String::new();

        // Add icon
        buf.push_str(&format!("{} ", self.get_icon(true)));

        // Add timestamp
        let timestamp_str = format!("[{}] ", self.get_timestamp());
        buf.push_str(&timestamp_str.dimmed().to_string());

        // Add title with appropriate styling
        let title = match self.inner.category {
            Category::Action => self.inner.title.white(),
            Category::Info => self.inner.title.white(),
            Category::Debug => self.inner.title.dimmed(),
            Category::Error => format!("{} {}", "ERROR:".bold(), self.inner.title).red(),
            Category::Completion => self.inner.title.white().bold(),
            Category::Warning => {
                format!("{} {}", "WARNING:".bold(), self.inner.title).bright_yellow()
            }
        };

        buf.push_str(&title.to_string());

        // Add subtitle if present
        if let Some(ref sub_title) = self.inner.sub_title {
            buf.push_str(&format!(" {}", sub_title.dimmed()));
        }

        buf
    }

    fn format_plain(&self) -> String {
        let mut buf = String::new();

        // Add icon (plain)
        buf.push_str(&format!("{} ", self.get_icon(false)));

        // Add timestamp
        buf.push_str(&format!("[{}] ", self.get_timestamp()));

        // Add title
        buf.push_str(&self.inner.title);

        // Add subtitle if present
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
    fn test_title_display_with_template_error_level() {
        let title = TitleFormat {
            title: "Connection Failed".to_string(),
            sub_title: Some("Timeout".to_string()),
            category: Category::Error,
            timestamp: chrono::Utc::now(),
        };

        let env = Environment { title_format: r#"{{#if (eq level "error")}}{{red "[ERROR]"}} {{bold title}} {{dimmed subtitle}}{{/if}}"#.to_string(), ..fake::Faker.fake() };

        let display = title.display_with_env(env).with_colors(false);
        let result = display.format_with_data(None, None);

        assert!(result.contains("[ERROR]"));
        assert!(result.contains("Connection Failed"));
        assert!(result.contains("Timeout"));
    }

    #[test]
    fn test_title_display_with_template_warning_level() {
        let title = TitleFormat {
            title: "High Memory Usage".to_string(),
            sub_title: None,
            category: Category::Warning,
            timestamp: chrono::Utc::now(),
        };

        let env = Environment {
            title_format:
                r#"{{#if (eq level "warning")}}{{bright_yellow "[WARNING]"}} {{bold title}}{{/if}}"#
                    .to_string(),
            ..fake::Faker.fake()
        };

        let display = title.display_with_env(env).with_colors(false);
        let result = display.format_with_data(None, None);

        assert!(result.contains("[WARNING]"));
        assert!(result.contains("High Memory Usage"));
    }

    #[test]
    fn test_title_display_with_usage_in_template() {
        let title = TitleFormat {
            title: "Task Complete".to_string(),
            sub_title: None,
            category: Category::Info,
            timestamp: chrono::Utc::now(),
        };

        let usage = Usage {
            prompt_tokens: TokenCount::Actual(1024),
            completion_tokens: TokenCount::Actual(2048),
            total_tokens: TokenCount::Actual(3072),
            cached_tokens: TokenCount::Actual(512),
            cost: Some(0.05),
        };

        let env = Environment {
            title_format: "{{input}}/{{output}} {{cost}} {{title}}".to_string(),
            ..fake::Faker.fake()
        };

        let display = title.display_with_env(env).with_colors(false);
        let result = display.format_with_data(Some(&usage), None);

        assert!(result.contains("1024/2048"));
        assert!(result.contains("$0.0500"));
        assert!(result.contains("Task Complete"));
    }

    #[test]
    fn test_title_display_with_conditional_template() {
        let title = TitleFormat {
            title: "Debug Info".to_string(),
            sub_title: None,
            category: Category::Debug,
            timestamp: chrono::Utc::now(),
        };

        let env = Environment {
            title_format:
                r#"{{#if (eq level "debug")}}{{dimmed title}}{{else}}{{white title}}{{/if}}"#
                    .to_string(),
            ..fake::Faker.fake()
        };

        let display = title.display_with_env(env).with_colors(false);
        let result = display.format_with_data(None, None);

        assert!(result.contains("Debug Info"));
    }

    #[test]
    fn test_title_display_without_env_uses_default() {
        let title = TitleFormat {
            title: "Test".to_string(),
            sub_title: None,
            category: Category::Info,
            timestamp: chrono::Utc::now(),
        };

        let display = title.display().with_colors(false);
        let result = format!("{}", display);

        assert!(result.contains("Test"));
    }
}
