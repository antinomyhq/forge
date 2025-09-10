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

    pub fn undo_operation(&mut self, lines_added: u64, lines_removed: u64) {
        self.lines_added = self.lines_added.saturating_sub(lines_added);
        self.lines_removed = self.lines_removed.saturating_sub(lines_removed);
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

    pub fn record_file_undo(&mut self, path: String, lines_added: u64, lines_removed: u64) {
        let file_metrics = self.files_changed.entry(path).or_default();
        file_metrics.undo_operation(lines_added, lines_removed);
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

    #[test]
    fn test_metrics_record_file_operation_and_undo() {
        let mut metrics = Metrics::new();
        let path = "file_to_track.rs".to_string();

        // Do operation
        metrics.record_file_operation(path.clone(), 2, 1);
        let changes = metrics.files_changed.get(&path).unwrap();
        assert_eq!(metrics.files_changed.len(), 1);
        assert_eq!(changes.lines_added, 2);
        assert_eq!(changes.lines_removed, 1);

        // Undo operation
        metrics.record_file_undo(path.clone(), 2, 1);
        let changes = metrics.files_changed.get(&path).unwrap();
        assert_eq!(changes.lines_added, 0);
        assert_eq!(changes.lines_removed, 0);
    }

    #[test]
    fn test_metrics_record_multiple_file_operations_and_undo() {
        let mut metrics = Metrics::new();
        let path = "file1.rs".to_string();

        metrics.record_file_operation(path.clone(), 10, 5);
        metrics.record_file_operation(path.clone(), 5, 1);

        let metric1 = metrics.files_changed.get(&path).unwrap();
        assert_eq!(metric1.lines_added, 15);
        assert_eq!(metric1.lines_removed, 6);

        // Undo operation on file1 (undoing the second operation: 5 added, 1 removed)
        metrics.record_file_undo(path.clone(), 5, 1);
        let file1_metrics_after_undo1 = metrics.files_changed.get(&path).unwrap();
        assert_eq!(file1_metrics_after_undo1.lines_added, 10);
        assert_eq!(file1_metrics_after_undo1.lines_removed, 5);

        // Undo operation on file1 (undoing the first operation: 10 added, 5 removed)
        metrics.record_file_undo(path.clone(), 10, 5);
        let file1_metrics_after_undo2 = metrics.files_changed.get(&path).unwrap();
        assert_eq!(file1_metrics_after_undo2.lines_added, 0);
        assert_eq!(file1_metrics_after_undo2.lines_removed, 0);
    }
}
