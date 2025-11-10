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
}
