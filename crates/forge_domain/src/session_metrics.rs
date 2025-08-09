use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use colored::Colorize;
use derive_setters::Setters;
use serde::{Deserialize, Serialize};

/// Tracks metrics for individual file changes
#[derive(Debug, Clone, Default, Setters, Serialize, Deserialize)]
#[setters(into, strip_option)]
pub struct FileChangeMetrics {
    pub lines_added: u64,
    pub lines_removed: u64,
    pub operations_count: u64,
}

impl FileChangeMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_operation(&mut self, lines_added: u64, lines_removed: u64) {
        self.lines_added += lines_added;
        self.lines_removed += lines_removed;
        self.operations_count += 1;
    }

    /// Gets the net change in lines for this file
    pub fn net_change(&self) -> i64 {
        self.lines_added as i64 - self.lines_removed as i64
    }
}

/// Aggregates conversation metrics including file changes, operations, and
/// duration
#[derive(Debug, Clone, Default, Setters, Serialize, Deserialize)]
#[setters(into, strip_option)]
pub struct Metrics {
    pub started_at: Option<DateTime<Utc>>,
    pub files_changed: HashMap<String, FileChangeMetrics>,
    pub total_lines_added: u64,
    pub total_lines_removed: u64,
    pub operations_count: u64,
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts tracking session metrics
    pub fn start(&mut self) {
        self.started_at = Some(Utc::now());
    }

    pub fn record_file_operation(&mut self, path: String, lines_added: u64, lines_removed: u64) {
        // Update file-specific metrics
        let file_metrics = self.files_changed.entry(path).or_default();
        file_metrics.add_operation(lines_added, lines_removed);

        // Update totals
        self.total_lines_added += lines_added;
        self.total_lines_removed += lines_removed;
        self.operations_count += 1;
    }

    /// Gets the session duration if tracking has started
    pub fn duration(&self) -> Option<Duration> {
        self.started_at
            .map(|start| (Utc::now() - start).to_std().unwrap_or_default())
    }

    pub fn files_changed_count(&self) -> usize {
        self.files_changed.len()
    }

    pub fn net_change(&self) -> i64 {
        self.total_lines_added as i64 - self.total_lines_removed as i64
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

/// Formatted session summary for display
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub duration: String,
    pub files_changed: usize,
    pub lines_added: u64,
    pub lines_removed: u64,
    pub net_change: i64,
    pub operations: u64,
}

impl From<&Metrics> for SessionSummary {
    fn from(metrics: &Metrics) -> Self {
        let duration = match metrics.duration() {
            Some(d) => {
                let total_seconds = d.as_secs();
                let minutes = total_seconds / 60;
                let seconds = total_seconds % 60;
                format!("{}m {}s", minutes, seconds)
            }
            None => "0m 0s".to_string(),
        };

        Self {
            duration,
            files_changed: metrics.files_changed_count(),
            lines_added: metrics.total_lines_added,
            lines_removed: metrics.total_lines_removed,
            net_change: metrics.net_change(),
            operations: metrics.operations_count,
        }
    }
}

impl std::fmt::Display for SessionSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", "SESSION SUMMARY".bold().dimmed())?;
        writeln!(f, "{}: {}", "Duration".bright_cyan().bold(), self.duration)?;
        writeln!(
            f,
            "{}: {}",
            "Files Changed".bright_cyan().bold(),
            self.files_changed
        )?;
        writeln!(
            f,
            "{}: {}",
            "Lines Added".bright_cyan().bold(),
            self.lines_added.to_string().green()
        )?;
        writeln!(
            f,
            "{}: {}",
            "Lines Removed".bright_cyan().bold(),
            self.lines_removed.to_string().red()
        )?;

        let colored_net_change = if self.net_change >= 0 {
            format!("{:+}", self.net_change).green()
        } else {
            format!("{}", self.net_change).red()
        };

        writeln!(
            f,
            "{}: {}",
            "Net Change (lines)".bright_cyan().bold(),
            colored_net_change
        )?;
        writeln!(
            f,
            "{}: {}",
            "Operations".bright_cyan().bold(),
            self.operations
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_file_change_metrics_new() {
        let fixture = FileChangeMetrics::new();
        let actual = fixture;
        let expected = FileChangeMetrics { lines_added: 0, lines_removed: 0, operations_count: 0 };
        assert_eq!(actual.lines_added, expected.lines_added);
        assert_eq!(actual.lines_removed, expected.lines_removed);
        assert_eq!(actual.operations_count, expected.operations_count);
    }

    #[test]
    fn test_file_change_metrics_add_operation() {
        let mut fixture = FileChangeMetrics::new();
        fixture.add_operation(10, 5);
        fixture.add_operation(3, 2);

        let actual = fixture;
        let expected = FileChangeMetrics { lines_added: 13, lines_removed: 7, operations_count: 2 };
        assert_eq!(actual.lines_added, expected.lines_added);
        assert_eq!(actual.lines_removed, expected.lines_removed);
        assert_eq!(actual.operations_count, expected.operations_count);
    }

    #[test]
    fn test_file_change_metrics_net_change() {
        let mut fixture = FileChangeMetrics::new();
        fixture.add_operation(10, 5);

        let actual = fixture.net_change();
        let expected = 5;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_metrics_new() {
        let fixture = Metrics::new();
        let actual = fixture;

        assert_eq!(actual.files_changed.len(), 0);
        assert_eq!(actual.total_lines_added, 0);
        assert_eq!(actual.total_lines_removed, 0);
        assert_eq!(actual.operations_count, 0);
    }

    #[test]
    fn test_metrics_record_file_operation() {
        let mut fixture = Metrics::new();
        fixture.record_file_operation("file1.rs".to_string(), 10, 5);
        fixture.record_file_operation("file2.rs".to_string(), 3, 2);
        fixture.record_file_operation("file1.rs".to_string(), 5, 1);

        let actual = fixture;

        assert_eq!(actual.files_changed_count(), 2);
        assert_eq!(actual.total_lines_added, 18);
        assert_eq!(actual.total_lines_removed, 8);
        assert_eq!(actual.operations_count, 3);

        let file1_metrics = actual.files_changed.get("file1.rs").unwrap();
        assert_eq!(file1_metrics.lines_added, 15);
        assert_eq!(file1_metrics.lines_removed, 6);
        assert_eq!(file1_metrics.operations_count, 2);
    }

    #[test]
    fn test_metrics_net_change() {
        let mut fixture = Metrics::new();
        fixture.record_file_operation("file1.rs".to_string(), 10, 5);

        let actual = fixture.net_change();
        let expected = 5;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_session_summary_from_metrics() {
        let mut fixture = Metrics::new();
        fixture.start();
        fixture.record_file_operation("file1.rs".to_string(), 247, 89);
        fixture.record_file_operation("file2.rs".to_string(), 50, 20);

        let actual = SessionSummary::from(&fixture);

        assert_eq!(actual.files_changed, 2);
        assert_eq!(actual.lines_added, 297);
        assert_eq!(actual.lines_removed, 109);
        assert_eq!(actual.net_change, 188);
        assert_eq!(actual.operations, 2);
    }
}
