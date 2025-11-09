use std::iter::Sum;
use std::ops::Add;

use derive_setters::Setters;
use serde::{Deserialize, Serialize};

use crate::ToolKind;

/// Tracks metrics for individual file changes
#[derive(Debug, Clone, PartialEq, Setters, Serialize, Deserialize)]
#[setters(into)]
pub struct FileOperation {
    pub lines_added: u64,
    pub lines_removed: u64,
    /// Content hash of the file. None if file is unreadable (deleted, no
    /// permissions, etc.)
    pub content_hash: Option<String>,
    /// The tool that performed this operation
    pub tool: ToolKind,
}

impl FileOperation {
    /// Creates a new FileChangeMetrics with the specified tool
    /// Other fields default to zero/None and can be set using setters
    pub fn new(tool: ToolKind) -> Self {
        Self { lines_added: 0, lines_removed: 0, content_hash: None, tool }
    }

    /// Aggregates multiple file change metrics into a single metric
    /// The resulting metric will have the sum of all lines added/removed
    /// and will use the content hash from the last operation
    pub fn aggregate(metrics: &[FileOperation]) -> Option<Self> {
        if metrics.is_empty() {
            return None;
        }

        Some(metrics.iter().cloned().sum())
    }
}

impl Add for FileOperation {
    type Output = Self;

    /// Adds two FileChangeMetrics together
    /// The resulting metric will have the sum of lines added/removed
    /// and will use the content hash and tool from the right-hand side
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            lines_added: self.lines_added + rhs.lines_added,
            lines_removed: self.lines_removed + rhs.lines_removed,
            content_hash: rhs.content_hash,
            tool: rhs.tool,
        }
    }
}

impl Sum for FileOperation {
    /// Sums an iterator of FileChangeMetrics
    /// Returns a default FileChangeMetrics if the iterator is empty
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(FileOperation::new(ToolKind::Write), |acc, x| acc + x)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_file_change_metrics_new() {
        let actual = FileOperation::new(ToolKind::Write)
            .lines_added(10u64)
            .lines_removed(5u64)
            .content_hash(Some("abc123".to_string()));

        let expected = FileOperation {
            lines_added: 10,
            lines_removed: 5,
            content_hash: Some("abc123".to_string()),
            tool: ToolKind::Write,
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_file_change_metrics_add() {
        let first = FileOperation::new(ToolKind::Write)
            .lines_added(10u64)
            .lines_removed(5u64)
            .content_hash(Some("hash1".to_string()));

        let second = FileOperation::new(ToolKind::Patch)
            .lines_added(20u64)
            .lines_removed(3u64)
            .content_hash(Some("hash2".to_string()));

        let actual = first + second;

        let expected = FileOperation {
            lines_added: 30,
            lines_removed: 8,
            content_hash: Some("hash2".to_string()),
            tool: ToolKind::Patch,
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_file_change_metrics_aggregate() {
        let metrics = vec![
            FileOperation::new(ToolKind::Write)
                .lines_added(10u64)
                .lines_removed(5u64)
                .content_hash(Some("hash1".to_string())),
            FileOperation::new(ToolKind::Patch)
                .lines_added(20u64)
                .lines_removed(3u64)
                .content_hash(Some("hash2".to_string())),
            FileOperation::new(ToolKind::Patch)
                .lines_added(5u64)
                .lines_removed(2u64)
                .content_hash(Some("hash3".to_string())),
        ];

        let actual = FileOperation::aggregate(&metrics).unwrap();

        let expected = FileOperation {
            lines_added: 35,
            lines_removed: 10,
            content_hash: Some("hash3".to_string()),
            tool: ToolKind::Patch,
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_file_change_metrics_aggregate_empty() {
        let metrics = vec![];
        let actual = FileOperation::aggregate(&metrics);
        assert_eq!(actual, None);
    }
}
