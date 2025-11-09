use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use derive_setters::Setters;
use serde::{Deserialize, Serialize};

use crate::ToolKind;

/// Tracks metrics for individual file changes
#[derive(Debug, Clone, PartialEq, Setters, Serialize, Deserialize)]
#[setters(into)]
pub struct FileChangeMetrics {
    pub lines_added: u64,
    pub lines_removed: u64,
    /// Content hash of the file. None if file is unreadable (deleted, no
    /// permissions, etc.)
    pub file_hash: Option<String>, // FIXME: Rename to content-hash
    /// The tool that performed this operation
    pub tool: ToolKind,
}

impl FileChangeMetrics {
    /// Creates a new FileChangeMetrics with the specified tool
    /// Other fields default to zero/None and can be set using setters
    pub fn new(tool: ToolKind) -> Self {
        Self { lines_added: 0, lines_removed: 0, file_hash: None, tool }
    }
}

#[derive(Debug, Clone, Default, Setters, Serialize, Deserialize)]
#[setters(into, strip_option)]
pub struct Metrics {
    pub started_at: Option<DateTime<Utc>>,
    pub files_changed: HashMap<String, Vec<FileChangeMetrics>>,
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a file operation by adding it to the history
    pub fn add(mut self, path: String, metrics: FileChangeMetrics) -> Self {
        self.files_changed.entry(path).or_default().push(metrics);
        self
    }

    /// Gets the session duration if tracking has started
    /// Gets the session duration if tracking has started
    pub fn duration(&self, now: DateTime<Utc>) -> Option<Duration> {
        self.started_at
            .map(|start| (now - start).to_std().unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_file_change_metrics_new() {
        let actual = FileChangeMetrics::new(ToolKind::Write)
            .lines_added(10u64)
            .lines_removed(5u64)
            .file_hash(Some("abc123".to_string()));

        let expected = FileChangeMetrics {
            lines_added: 10,
            lines_removed: 5,
            file_hash: Some("abc123".to_string()),
            tool: ToolKind::Write,
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_metrics_new() {
        let actual = Metrics::new();
        assert_eq!(actual.files_changed.len(), 0);
    }

    #[test]
    fn test_metrics_record_file_operation() {
        let fixture = Metrics::new()
            .add(
                "file1.rs".to_string(),
                FileChangeMetrics::new(ToolKind::Write)
                    .lines_added(10u64)
                    .lines_removed(5u64)
                    .file_hash(Some("hash1".to_string())),
            )
            .add(
                "file2.rs".to_string(),
                FileChangeMetrics::new(ToolKind::Patch)
                    .lines_added(3u64)
                    .lines_removed(2u64)
                    .file_hash(Some("hash2".to_string())),
            )
            .add(
                "file1.rs".to_string(),
                FileChangeMetrics::new(ToolKind::Patch)
                    .lines_added(5u64)
                    .lines_removed(1u64)
                    .file_hash(Some("hash1_v2".to_string())),
            );

        let actual = fixture;

        // Check file1 has 2 operations recorded
        let file1_metrics = actual.files_changed.get("file1.rs").unwrap();
        assert_eq!(file1_metrics.len(), 2);
        assert_eq!(file1_metrics[0].lines_added, 10);
        assert_eq!(file1_metrics[0].lines_removed, 5);
        assert_eq!(file1_metrics[1].lines_added, 5);
        assert_eq!(file1_metrics[1].lines_removed, 1);

        // Check file2 has 1 operation recorded
        let file2_metrics = actual.files_changed.get("file2.rs").unwrap();
        assert_eq!(file2_metrics.len(), 1);
        assert_eq!(file2_metrics[0].lines_added, 3);
        assert_eq!(file2_metrics[0].lines_removed, 2);
    }

    #[test]
    fn test_metrics_record_file_operation_and_undo() {
        let path = "file_to_track.rs".to_string();

        // Do operation
        let metrics = Metrics::new().add(
            path.clone(),
            FileChangeMetrics::new(ToolKind::Write)
                .lines_added(2u64)
                .lines_removed(1u64)
                .file_hash(Some("hash_v1".to_string())),
        );
        let changes = metrics.files_changed.get(&path).unwrap();
        assert_eq!(metrics.files_changed.len(), 1);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].lines_added, 2);
        assert_eq!(changes[0].lines_removed, 1);
        assert_eq!(changes[0].file_hash, Some("hash_v1".to_string()));

        // Undo operation
        let metrics = metrics.add(
            path.clone(),
            FileChangeMetrics::new(ToolKind::Undo).file_hash(Some("hash_v0".to_string())),
        );
        let changes = metrics.files_changed.get(&path).unwrap();
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[1].lines_added, 0);
        assert_eq!(changes[1].lines_removed, 0);
        assert_eq!(changes[1].file_hash, Some("hash_v0".to_string()));
    }

    #[test]
    fn test_metrics_record_multiple_file_operations() {
        let path = "file1.rs".to_string();

        let metrics = Metrics::new()
            .add(
                path.clone(),
                FileChangeMetrics::new(ToolKind::Write)
                    .lines_added(10u64)
                    .lines_removed(5u64)
                    .file_hash(Some("hash1".to_string())),
            )
            .add(
                path.clone(),
                FileChangeMetrics::new(ToolKind::Patch)
                    .lines_added(5u64)
                    .lines_removed(1u64)
                    .file_hash(Some("hash2".to_string())),
            )
            .add(
                path.clone(),
                FileChangeMetrics::new(ToolKind::Undo).file_hash(Some("hash1".to_string())),
            );

        let operations = metrics.files_changed.get(&path).unwrap();
        assert_eq!(operations.len(), 3);

        // First operation
        assert_eq!(operations[0].lines_added, 10);
        assert_eq!(operations[0].lines_removed, 5);
        assert_eq!(operations[0].file_hash, Some("hash1".to_string()));

        // Second operation
        assert_eq!(operations[1].lines_added, 5);
        assert_eq!(operations[1].lines_removed, 1);
        assert_eq!(operations[1].file_hash, Some("hash2".to_string()));

        // Third operation (undo)
        assert_eq!(operations[2].lines_added, 0);
        assert_eq!(operations[2].lines_removed, 0);
        assert_eq!(operations[2].file_hash, Some("hash1".to_string()));
    }
}
