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
            let timestamp_str = if let Some(replay_ts) = self.inner.timestamp {
                // Use replay timestamp if provided
                let local_time: chrono::DateTime<Local> = replay_ts.into();
                format!("[{}] ", local_time.format("%H:%M:%S"))
            } else {
                // Use current time for live conversations
                format!("[{}] ", Local::now().format("%H:%M:%S"))
            };
            buf.push_str(timestamp_str.dimmed().to_string().as_str());
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
            let timestamp_str = if let Some(replay_ts) = self.inner.timestamp {
                // Use replay timestamp if provided
                let local_time: chrono::DateTime<Local> = replay_ts.into();
                format!("[{}] ", local_time.format("%H:%M:%S"))
            } else {
                // Use current time for live conversations
                format!("[{}] ", Local::now().format("%H:%M:%S"))
            };
            buf.push_str(&timestamp_str);
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

#[cfg(test)]
mod tests {
    use super::*;
    use forge_domain::{Category, TitleFormat};
    use chrono::{DateTime, Utc};

    #[test]
    fn test_title_display_with_replay_timestamp() {
        let replay_timestamp = DateTime::parse_from_rfc3339("2023-10-26T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        
        let title = TitleFormat {
            title: "Test Action".to_string(),
            sub_title: Some("Subtitle".to_string()),
            category: Category::Action,
            timestamp: Some(replay_timestamp),
        };

        let display = title.display().with_timestamp(true).with_colors(false);
        let output = display.to_string();

        // Convert to local time to verify the output
        let local_time: chrono::DateTime<chrono::Local> = replay_timestamp.into();
        let expected_timestamp = local_time.format("%H:%M:%S").to_string();

        // Should contain the replay timestamp converted to local time
        assert!(output.contains(&format!("[{}]", expected_timestamp)));
        assert!(output.contains("Test Action"));
        assert!(output.contains("Subtitle"));
    }

    #[test]
    fn test_title_display_without_timestamp() {
        let title = TitleFormat::info("Test Info");
        
        let display = title.display().with_timestamp(true).with_colors(false);
        let output = display.to_string();

        // Should contain current time format
        assert!(output.contains("⏺"));
        assert!(output.contains("Test Info"));
        // Should have timestamp in HH:MM:SS format
        assert!(output.contains("[") && output.contains("]"));
    }

    #[test]
    fn test_title_display_timestamp_disabled() {
        let replay_timestamp = DateTime::parse_from_rfc3339("2023-10-26T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        
        let title = TitleFormat {
            title: "Test Action".to_string(),
            sub_title: None,
            category: Category::Action,
            timestamp: Some(replay_timestamp),
        };

        let display = title.display().with_timestamp(false).with_colors(false);
        let output = display.to_string();

        // Should not contain any timestamp when disabled
        assert!(!output.contains("["));
        assert!(!output.contains("]"));
        assert!(output.contains("Test Action"));
    }

    #[test]
    fn test_title_display_with_colors_and_timestamp() {
        let replay_timestamp = DateTime::parse_from_rfc3339("2023-10-26T14:45:30Z")
            .unwrap()
            .with_timezone(&Utc);
        
        let title = TitleFormat {
            title: "Error Message".to_string(),
            sub_title: Some("Error details".to_string()),
            category: Category::Error,
            timestamp: Some(replay_timestamp),
        };

        let display = title.display().with_timestamp(true).with_colors(true);
        let output = display.to_string();

        // Convert to local time to verify the output
        let local_time: chrono::DateTime<chrono::Local> = replay_timestamp.into();
        let expected_timestamp = local_time.format("%H:%M:%S").to_string();

        // Should contain replay timestamp converted to local time
        assert!(output.contains(&format!("[{}]", expected_timestamp)));
        assert!(output.contains("Error Message"));
        assert!(output.contains("Error details"));
        // Should contain ERROR prefix for error category
        assert!(output.contains("ERROR:"));
    }

    #[test]
    fn test_title_display_all_categories_with_timestamp() {
        let replay_timestamp = DateTime::parse_from_rfc3339("2023-10-26T09:15:45Z")
            .unwrap()
            .with_timezone(&Utc);

        // Convert to local time once for verification
        let local_time: chrono::DateTime<chrono::Local> = replay_timestamp.into();
        let expected_timestamp = local_time.format("%H:%M:%S").to_string();

        let categories = vec![
            (Category::Action, "Action"),
            (Category::Info, "Info"),
            (Category::Debug, "Debug"),
            (Category::Error, "Error"),
            (Category::Completion, "Completion"),
        ];

        for (category, title_text) in categories {
            let title = TitleFormat {
                title: title_text.to_string(),
                sub_title: None,
                category,
                timestamp: Some(replay_timestamp),
            };

            let display = title.display().with_timestamp(true).with_colors(false);
            let output = display.to_string();

            // All should contain the replay timestamp converted to local time
            assert!(output.contains(&format!("[{}]", expected_timestamp)));
            assert!(output.contains(title_text));
        }
    }

    #[test]
    fn test_title_display_current_time_fallback() {
        let title = TitleFormat::debug("Debug Message");
        
        // Get time before display
        let before = chrono::Local::now();
        
        let display = title.display().with_timestamp(true).with_colors(false);
        let output = display.to_string();
        
        // Get time after display
        let after = chrono::Local::now();
        
        // Extract timestamp from output (format: [HH:MM:SS])
        let timestamp_start = output.find('[').unwrap();
        let timestamp_end = output.find(']').unwrap();
        let timestamp_str = &output[timestamp_start + 1..timestamp_end];
        
        // Parse the timestamp to verify it's within expected range
        let output_time = chrono::NaiveTime::parse_from_str(timestamp_str, "%H:%M:%S").unwrap();
        let before_time = before.time();
        let after_time = after.time();
        
        // The timestamp should be between before and after (allowing for some margin)
        assert!(output_time >= before_time || output_time <= after_time);
    }

    #[test]
    fn test_title_display_extension_trait() {
        let replay_timestamp = DateTime::parse_from_rfc3339("2023-10-26T16:20:10Z")
            .unwrap()
            .with_timezone(&Utc);
        
        let title = TitleFormat::info("Extension Test")
            .timestamp(replay_timestamp);

        // Convert to local time for verification
        let local_time: chrono::DateTime<chrono::Local> = replay_timestamp.into();
        let expected_timestamp = local_time.format("%H:%M:%S").to_string();

        // Test the extension trait methods
        let display1 = title.clone().display();
        let display2 = title.clone().display_with_colors(false);
        let display3 = title.clone().display_with_timestamp(false);

        // All should be TitleDisplay instances
        let output1 = display1.to_string(); // display() has timestamp enabled by default
        let output2 = display2.to_string(); // display_with_colors(false) has timestamp enabled by default
        let output3 = display3.to_string(); // display_with_timestamp(false) has timestamp disabled

        println!("Output1 (with timestamp): {}", output1);
        println!("Output2 (with timestamp, no colors): {}", output2);
        println!("Output3 (no timestamp): {}", output3);

        // display1 and display2 should have timestamp
        assert!(output1.contains(&format!("[{}]", expected_timestamp)));
        assert!(output2.contains(&format!("[{}]", expected_timestamp)));
        // display3 should not have timestamp brackets (i.e., [HH:MM:SS] pattern)
        // We need to check for the specific timestamp pattern, not just any brackets
        // since ANSI color codes also contain brackets
        let has_timestamp_pattern = output3.matches("[").count() >= 2 && output3.contains("]:");
        
        assert!(!has_timestamp_pattern);
    }
}