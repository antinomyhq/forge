use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Permission types that can be applied to operations
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Permission {
    /// Allow the operation without asking
    Allow,
    /// Deny the operation without asking
    Deny,
    /// Confirm with the user before allowing
    Confirm,
}
