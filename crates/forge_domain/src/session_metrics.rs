use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use derive_setters::Setters;
use serde::{Deserialize, Serialize};

/// Tracks metrics for individual file changes
#[derive(Debug, Clone, Default, Setters, Serialize, Deserialize)]
#[setters(into, strip_option)]
pub struct FileChangeMetrics {
    pub lines_added: u64,
    pub lines_removed: u64,
}

impl FileChangeMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_operation(&mut self, lines_added: u64, lines_removed: u64) {
        self.lines_added += lines_added;
        self.lines_removed += lines_removed;
    }
}

#[derive(Debug, Clone, Default, Setters, Serialize, Deserialize)]
#[setters(into, strip_option)]
pub struct Metrics {
    pub started_at: Option<DateTime<Utc>>,
    pub files_changed: HashMap<String, FileChangeMetrics>,
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
    }

    /// Gets the session duration if tracking has started
    pub fn duration(&self) -> Option<Duration> {
        self.started_at
            .map(|start| (Utc::now() - start).to_std().unwrap_or_default())
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
        let expected = FileChangeMetrics { lines_added: 0, lines_removed: 0 };
        assert_eq!(actual.lines_added, expected.lines_added);
        assert_eq!(actual.lines_removed, expected.lines_removed);
    }

    #[test]
    fn test_file_change_metrics_add_operation() {
        let mut fixture = FileChangeMetrics::new();
        fixture.add_operation(10, 5);
        fixture.add_operation(3, 2);

        let actual = fixture;
        let expected = FileChangeMetrics { lines_added: 13, lines_removed: 7 };
        assert_eq!(actual.lines_added, expected.lines_added);
        assert_eq!(actual.lines_removed, expected.lines_removed);
    }

    #[test]
    fn test_metrics_new() {
        let fixture = Metrics::new();
        let actual = fixture;

        assert_eq!(actual.files_changed.len(), 0);
    }

    #[test]
    fn test_metrics_record_file_operation() {
        let mut fixture = Metrics::new();
        fixture.record_file_operation("file1.rs".to_string(), 10, 5);
        fixture.record_file_operation("file2.rs".to_string(), 3, 2);
        fixture.record_file_operation("file1.rs".to_string(), 5, 1);

        let actual = fixture;

        let file1_metrics = actual.files_changed.get("file1.rs").unwrap();
        assert_eq!(file1_metrics.lines_added, 15);
        assert_eq!(file1_metrics.lines_removed, 6);
    }
}
