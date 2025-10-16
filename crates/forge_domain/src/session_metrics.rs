use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use derive_setters::Setters;
use serde::{Deserialize, Serialize};

/// Represents the state of a file at a specific point in time
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileState {
    /// File is readable with its content hash
    Readable { hash: String },
    /// File cannot be read (deleted, no permissions, etc.)
    Unreadable,
}

/// Tracks metrics for individual file changes
#[derive(Debug, Clone, Default, Setters, Serialize, Deserialize)]
#[setters(into, strip_option)]
pub struct FileChangeMetrics {
    pub lines_added: u64,
    pub lines_removed: u64,
    pub file_hash: String,
    /// The file state that was last notified to the agent about external
    /// changes. None if we've never notified the agent about external
    /// changes for this file.
    #[serde(default)]
    pub last_notified_state: Option<FileState>,
}

impl FileChangeMetrics {
    pub fn new(file_hash: String) -> Self {
        Self { file_hash, ..Default::default() }
    }

    pub fn add_operation(&mut self, lines_added: u64, lines_removed: u64, file_hash: String) {
        self.lines_added += lines_added;
        self.lines_removed += lines_removed;
        self.file_hash = file_hash;
    }

    pub fn undo_operation(&mut self, lines_added: u64, lines_removed: u64, file_hash: String) {
        self.lines_added = self.lines_added.saturating_sub(lines_added);
        self.lines_removed = self.lines_removed.saturating_sub(lines_removed);
        self.file_hash = file_hash;
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
    pub fn with_time(mut self, started_at: DateTime<Utc>) -> Self {
        self.started_at = Some(started_at);
        self
    }

    pub fn record_file_operation(
        &mut self,
        path: String,
        file_hash: String,
        lines_added: u64,
        lines_removed: u64,
    ) {
        // Update file-specific metrics
        let file_metrics = self.files_changed.entry(path).or_default();
        file_metrics.add_operation(lines_added, lines_removed, file_hash);
    }

    pub fn record_file_undo(
        &mut self,
        path: String,
        file_hash: String,
        lines_added: u64,
        lines_removed: u64,
    ) {
        let file_metrics = self.files_changed.entry(path).or_default();
        file_metrics.undo_operation(lines_added, lines_removed, file_hash);
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
        let fixture = FileChangeMetrics::new("abc123".to_string());
        let actual = fixture;
        let expected = FileChangeMetrics {
            lines_added: 0,
            lines_removed: 0,
            file_hash: "abc123".to_string(),
            last_notified_state: None,
        };
        assert_eq!(actual.lines_added, expected.lines_added);
        assert_eq!(actual.lines_removed, expected.lines_removed);
        assert_eq!(actual.file_hash, expected.file_hash);
    }

    #[test]
    fn test_file_change_metrics_add_operation() {
        let mut fixture = FileChangeMetrics::new("hash1".to_string());
        fixture.add_operation(10, 5, "hash2".to_string());
        fixture.add_operation(3, 2, "hash3".to_string());

        let actual = fixture;
        let expected = FileChangeMetrics {
            lines_added: 13,
            lines_removed: 7,
            file_hash: "hash3".to_string(),
            last_notified_state: None,
        };
        assert_eq!(actual.lines_added, expected.lines_added);
        assert_eq!(actual.lines_removed, expected.lines_removed);
        assert_eq!(actual.file_hash, expected.file_hash);
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
        fixture.record_file_operation("file1.rs".to_string(), "hash1".to_string(), 10, 5);
        fixture.record_file_operation("file2.rs".to_string(), "hash2".to_string(), 3, 2);
        fixture.record_file_operation("file1.rs".to_string(), "hash1_v2".to_string(), 5, 1);

        let actual = fixture;

        let file1_metrics = actual.files_changed.get("file1.rs").unwrap();
        assert_eq!(file1_metrics.lines_added, 15);
        assert_eq!(file1_metrics.lines_removed, 6);
        assert_eq!(file1_metrics.file_hash, "hash1_v2");
    }

    #[test]
    fn test_metrics_record_file_operation_and_undo() {
        let mut metrics = Metrics::new();
        let path = "file_to_track.rs".to_string();

        // Do operation
        metrics.record_file_operation(path.clone(), "hash_v1".to_string(), 2, 1);
        let changes = metrics.files_changed.get(&path).unwrap();
        assert_eq!(metrics.files_changed.len(), 1);
        assert_eq!(changes.lines_added, 2);
        assert_eq!(changes.lines_removed, 1);
        assert_eq!(changes.file_hash, "hash_v1");

        // Undo operation
        metrics.record_file_undo(path.clone(), "hash_v0".to_string(), 2, 1);
        let changes = metrics.files_changed.get(&path).unwrap();
        assert_eq!(changes.lines_added, 0);
        assert_eq!(changes.lines_removed, 0);
        assert_eq!(changes.file_hash, "hash_v0");
    }

    #[test]
    fn test_metrics_record_multiple_file_operations_and_undo() {
        let mut metrics = Metrics::new();
        let path = "file1.rs".to_string();

        metrics.record_file_operation(path.clone(), "hash1".to_string(), 10, 5);
        metrics.record_file_operation(path.clone(), "hash2".to_string(), 5, 1);

        let metric1 = metrics.files_changed.get(&path).unwrap();
        assert_eq!(metric1.lines_added, 15);
        assert_eq!(metric1.lines_removed, 6);
        assert_eq!(metric1.file_hash, "hash2");

        // Undo operation on file1 (undoing the second operation: 5 added, 1 removed)
        metrics.record_file_undo(path.clone(), "hash1".to_string(), 5, 1);
        let file1_metrics_after_undo1 = metrics.files_changed.get(&path).unwrap();
        assert_eq!(file1_metrics_after_undo1.lines_added, 10);
        assert_eq!(file1_metrics_after_undo1.lines_removed, 5);
        assert_eq!(file1_metrics_after_undo1.file_hash, "hash1");

        // Undo operation on file1 (undoing the first operation: 10 added, 5 removed)
        metrics.record_file_undo(path.clone(), "hash0".to_string(), 10, 5);
        let file1_metrics_after_undo2 = metrics.files_changed.get(&path).unwrap();
        assert_eq!(file1_metrics_after_undo2.lines_added, 0);
        assert_eq!(file1_metrics_after_undo2.lines_removed, 0);
        assert_eq!(file1_metrics_after_undo2.file_hash, "hash0");
    }
}
