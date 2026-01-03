use serde::{Deserialize, Serialize};

/// Represents a match found by fuzzy search
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchMatch {
    /// Start line number (1-based)
    pub start_line: u32,
    /// End line number (1-based)
    pub end_line: u32,
}
