use std::fmt;

use colored::Colorize;
use console::strip_ansi_codes;
use derive_setters::Setters;
use forge_api::Usage;
use forge_domain::{Category, TitleFormat};

use crate::format_utils::humanize_number;

/// Implementation of Display for TitleFormat in the presentation layer
#[derive(Setters)]
pub struct TitleDisplay {
    title: TitleFormat,
    colors: bool,
    usage: Option<Usage>,
    total_cost: Option<f64>,
}

impl TitleDisplay {
    pub fn new(title: TitleFormat) -> Self {
        Self {
            title,
            colors: true,
            usage: Default::default(),
            total_cost: Default::default(),
        }
    }

    fn format_with_colors(&self) -> String {
        let mut buf = String::new();

        let icon = match self.title.category {
            Category::Action => "⏺".yellow(),
            Category::Info => "⏺".white(),
            Category::Debug => "⏺".cyan(),
            Category::Error => "⏺".red(),
            Category::Completion => "⏺".yellow(),
            Category::Warning => "⚠️".bright_yellow(),
        };

        buf.push_str(format!("{icon} ").as_str());

        let mut timestamp_str = format!("{}", self.title.timestamp.format("%H:%M:%S"));

        // Add usage information if available
        if let Some(usage) = &self.usage {
            let total_tokens = *usage.total_tokens;
            if total_tokens > 0 {
                let humanized_tokens = humanize_number(total_tokens);
                timestamp_str.push_str(&format!(" {}", humanized_tokens));

                // Add cost if available
                if let Some(cost) = usage.cost {
                    timestamp_str.push_str(&format!(" ${:.4}", cost));
                }

                // Add cache percentage if there are cached tokens
                let cached = *usage.cached_tokens;
                if cached > 0 {
                    let cache_pct = (cached as f64 / total_tokens as f64) * 100.0;
                    timestamp_str.push_str(&format!(" {}% cached", cache_pct.round() as u32));
                }
            }
        }

        let timestamp_str = format!("[{}] ", timestamp_str);
        buf.push_str(timestamp_str.dimmed().to_string().as_str());

        let title = match self.title.category {
            Category::Action => self.title.title.white(),
            Category::Info => self.title.title.white(),
            Category::Debug => self.title.title.dimmed(),
            Category::Error => format!("{} {}", "ERROR:".bold(), self.title.title).red(),
            Category::Completion => self.title.title.white().bold(),
            Category::Warning => {
                format!("{} {}", "WARNING:".bold(), self.title.title).bright_yellow()
            }
        };

        buf.push_str(title.to_string().as_str());

        if let Some(ref sub_title) = self.title.sub_title {
            buf.push_str(&format!(" {}", sub_title.dimmed()).to_string());
        }

        buf
    }

}

impl fmt::Display for TitleDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.colors {
            write!(f, "{}", self.format_with_colors())
        } else {
            write!(f, "{}", strip_ansi_codes(&self.format_with_colors()))
        }
    }
}

/// Extension trait to easily convert TitleFormat to displayable form
pub trait TitleDisplayExt {
    fn display(self) -> TitleDisplay;
}

impl TitleDisplayExt for TitleFormat {
    fn display(self) -> TitleDisplay {
        TitleDisplay::new(self)
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use forge_domain::TokenCount;
    use pretty_assertions::assert_eq;

    use super::*;

    fn create_test_title() -> TitleFormat {
        let timestamp = chrono::DateTime::parse_from_rfc3339("2024-01-01T14:23:45Z")
            .unwrap()
            .with_timezone(&Utc);
        TitleFormat {
            title: "Test Title".to_string(),
            sub_title: None,
            category: Category::Action,
            timestamp,
        }
    }

    fn create_test_usage(total_tokens: usize, cached_tokens: usize, cost: Option<f64>) -> Usage {
        Usage {
            prompt_tokens: TokenCount::Actual(0),
            completion_tokens: TokenCount::Actual(0),
            total_tokens: TokenCount::Actual(total_tokens),
            cached_tokens: TokenCount::Actual(cached_tokens),
            cost,
        }
    }

    #[test]
    fn test_title_display_without_usage() {
        let title = create_test_title();
        let actual = TitleDisplay::new(title).colors(false).to_string();
        let expected = "⏺ [14:23:45] Test Title";

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_title_display_with_tokens_only() {
        let title = create_test_title();
        let usage = create_test_usage(1500, 0, None);
        let actual = TitleDisplay::new(title)
            .colors(false)
            .usage(Some(usage))
            .to_string();
        let expected = "⏺ [14:23:45 1.5k] Test Title";

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_title_display_with_tokens_and_cost() {
        let title = create_test_title();
        let usage = create_test_usage(2_300_000, 0, Some(0.0123));
        let actual = TitleDisplay::new(title)
            .colors(false)
            .usage(Some(usage))
            .to_string();
        let expected = "⏺ [14:23:45 2.3M $0.0123] Test Title";

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_title_display_with_all_metrics() {
        let title = create_test_title();
        let usage = create_test_usage(1000, 250, Some(0.0456));
        let actual = TitleDisplay::new(title)
            .colors(false)
            .usage(Some(usage))
            .to_string();
        let expected = "⏺ [14:23:45 1.0k $0.0456 25% cached] Test Title";

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_title_display_with_subtitle() {
        let mut title = create_test_title();
        title.sub_title = Some("Subtitle text".to_string());
        let usage = create_test_usage(2000, 100, Some(0.005));
        let actual = TitleDisplay::new(title)
            .colors(false)
            .usage(Some(usage))
            .to_string();
        let expected = "⏺ [14:23:45 2.0k $0.0050 5% cached] Test Title Subtitle text";

        assert_eq!(actual, expected);
    }
}
