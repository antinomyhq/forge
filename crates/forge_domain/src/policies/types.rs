use std::fmt::{Display, Formatter};

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

impl Display for Permission {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Permission::Allow => write!(f, "ALLOW"),
            Permission::Deny => write!(f, "DENY"),
            Permission::Confirm => write!(f, "CONFIRM"),
        }
    }
}
