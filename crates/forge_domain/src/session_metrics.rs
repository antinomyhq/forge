use std::collections::{HashMap, HashSet};
use std::time::Duration;

use chrono::{DateTime, Utc};
use derive_setters::Setters;
use serde::{Deserialize, Serialize};

pub use crate::file_operation::FileOperation;

#[derive(Debug, Clone, Default, Setters, Serialize, Deserialize)]
#[setters(into, strip_option)]
pub struct Metrics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,

    /// Holds the last file operation for each file
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub file_operations: HashMap<String, FileOperation>,

    /// Tracks all files that have been read in this session
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub files_accessed: HashSet<String>,
}

impl Metrics {
    /// Records a file operation, replacing any previous operation for the same file.
    /// Only Read operations are tracked in files_accessed.
    pub fn insert(mut self, path: String, metrics: FileOperation) -> Self {
        // Only track Read operations in files_accessed
        if metrics.tool == crate::ToolKind::Read {
            self.files_accessed.insert(path.clone());
        }
        self.file_operations.insert(path, metrics);
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
    use crate::ToolKind;

    #[test]
    fn test_metrics_new() {
        let actual = Metrics::default();
        assert_eq!(actual.file_operations.len(), 0);
    }

    #[test]
    fn test_metrics_record_file_operation() {
        let fixture = Metrics::default()
            .insert(
                "file1.rs".to_string(),
                FileOperation::new(ToolKind::Write)
                    .lines_added(10u64)
                    .lines_removed(5u64)
                    .content_hash(Some("hash1".to_string())),
            )
            .insert(
                "file2.rs".to_string(),
                FileOperation::new(ToolKind::Patch)
                    .lines_added(3u64)
                    .lines_removed(2u64)
                    .content_hash(Some("hash2".to_string())),
            )
            .insert(
                "file1.rs".to_string(),
                FileOperation::new(ToolKind::Patch)
                    .lines_added(5u64)
                    .lines_removed(1u64)
                    .content_hash(Some("hash1_v2".to_string())),
            );

        let actual = fixture;

        // Check file1 has the last operation recorded (second add overwrites the first)
        let file1_metrics = actual.file_operations.get("file1.rs").unwrap();
        assert_eq!(file1_metrics.lines_added, 5);
        assert_eq!(file1_metrics.lines_removed, 1);
        assert_eq!(file1_metrics.content_hash, Some("hash1_v2".to_string()));

        // Check file2 has its operation recorded
        let file2_metrics = actual.file_operations.get("file2.rs").unwrap();
        assert_eq!(file2_metrics.lines_added, 3);
        assert_eq!(file2_metrics.lines_removed, 2);
    }

    #[test]
    fn test_metrics_record_file_operation_and_undo() {
        let path = "file_to_track.rs".to_string();

        // Do operation
        let metrics = Metrics::default().insert(
            path.clone(),
            FileOperation::new(ToolKind::Write)
                .lines_added(2u64)
                .lines_removed(1u64)
                .content_hash(Some("hash_v1".to_string())),
        );
        let operation = metrics.file_operations.get(&path).unwrap();
        assert_eq!(metrics.file_operations.len(), 1);
        assert_eq!(operation.lines_added, 2);
        assert_eq!(operation.lines_removed, 1);
        assert_eq!(operation.content_hash, Some("hash_v1".to_string()));

        // Undo operation replaces the previous operation
        let metrics = metrics.insert(
            path.clone(),
            FileOperation::new(ToolKind::Undo).content_hash(Some("hash_v0".to_string())),
        );
        let operation = metrics.file_operations.get(&path).unwrap();
        assert_eq!(operation.lines_added, 0);
        assert_eq!(operation.lines_removed, 0);
        assert_eq!(operation.content_hash, Some("hash_v0".to_string()));
    }

    #[test]
    fn test_metrics_record_multiple_file_operations() {
        let path = "file1.rs".to_string();

        let metrics = Metrics::default()
            .insert(
                path.clone(),
                FileOperation::new(ToolKind::Write)
                    .lines_added(10u64)
                    .lines_removed(5u64)
                    .content_hash(Some("hash1".to_string())),
            )
            .insert(
                path.clone(),
                FileOperation::new(ToolKind::Patch)
                    .lines_added(5u64)
                    .lines_removed(1u64)
                    .content_hash(Some("hash2".to_string())),
            )
            .insert(
                path.clone(),
                FileOperation::new(ToolKind::Undo).content_hash(Some("hash1".to_string())),
            );

        // Only the last operation is stored
        let operation = metrics.file_operations.get(&path).unwrap();

        // Last operation (undo) overwrites previous operations
        assert_eq!(operation.lines_added, 0);
        assert_eq!(operation.lines_removed, 0);
        assert_eq!(operation.content_hash, Some("hash1".to_string()));
    }

    #[test]
    fn test_files_accessed_only_tracks_reads() {
        let metrics = Metrics::default()
            .insert(
                "file1.rs".to_string(),
                FileOperation::new(ToolKind::Read),
            )
            .insert(
                "file2.rs".to_string(),
                FileOperation::new(ToolKind::Write)
                    .lines_added(10u64),
            )
            .insert(
                "file3.rs".to_string(),
                FileOperation::new(ToolKind::Read),
            )
            .insert(
                "file3.rs".to_string(),
                FileOperation::new(ToolKind::Patch)
                    .lines_added(5u64),
            );

        // Only Read operations should be in files_accessed
        // file3 was read first, then patched - it stays in files_accessed
        assert_eq!(metrics.files_accessed.len(), 2);
        assert!(metrics.files_accessed.contains("file1.rs"));
        assert!(metrics.files_accessed.contains("file3.rs"));
        assert!(!metrics.files_accessed.contains("file2.rs")); // Write only, not in set

        // file_operations should have the last operation for each file
        assert_eq!(metrics.file_operations.len(), 3);
        assert_eq!(metrics.file_operations.get("file1.rs").unwrap().tool, ToolKind::Read);
        assert_eq!(metrics.file_operations.get("file2.rs").unwrap().tool, ToolKind::Write);
        assert_eq!(metrics.file_operations.get("file3.rs").unwrap().tool, ToolKind::Patch);
    }
}
