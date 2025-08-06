use derive_setters::Setters;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Permission types that can be applied to operations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Permission {
    /// Allow the operation without asking
    Allow,
    /// Disallow the operation without asking
    Disallow,
    /// Confirm with the user before allowing
    Confirm,
}

/// Trace information for policy evaluation
#[derive(Debug, Clone, PartialEq, Eq, Setters)]
#[setters(strip_option, into)]
pub struct Trace<T> {
    /// The actual value
    pub value: T,
    /// File where the policy is defined (for debugging)
    pub file: Option<std::path::PathBuf>,
    /// Line number in the file
    pub line: Option<u64>,
    /// Column start position
    pub col_start: Option<u64>,
    /// Column end position
    pub col_end: Option<u64>,
}

impl<T> Trace<T> {
    /// Create a new trace with just the value
    pub fn new(value: T) -> Self {
        Self {
            value,
            file: None,
            line: None,
            col_start: None,
            col_end: None,
        }
    }
}
