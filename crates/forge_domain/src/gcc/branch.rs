use derive_setters::Setters;
use serde::{Deserialize, Serialize};

use crate::gcc::{GccError, GccResult};

/// Represents a branch in the GCC context hierarchy.
#[derive(Debug, Clone, Serialize, Deserialize, Setters, PartialEq, Eq, Default)]
#[setters(strip_option, into)]
pub struct Branch {
    /// Unique name of the branch.
    pub name: String,
    /// Optional parent branch name (if this is a sub-branch).
    pub parent: Option<String>,
    /// Optional description of the branch.
    pub description: Option<String>,
}

impl Branch {
    /// Creates a new branch with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), ..Default::default() }
    }

    /// Validates the branch name is not empty.
    pub fn validate(&self) -> GccResult<()> {
        if self.name.trim().is_empty() {
            return Err(GccError::InvalidOperation(
                "Branch name cannot be empty".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn fixture_branch() -> Branch {
        Branch::new("feature-xyz")
            .parent("main")
            .description("A feature branch for XYZ")
    }

    #[test]
    fn test_branch_creation_and_validation() {
        let actual = fixture_branch();
        let expected = {
            let mut b = Branch::new("feature-xyz");
            b = b.parent("main");
            b = b.description("A feature branch for XYZ");
            b
        };
        assert_eq!(actual, expected);
        // validation should succeed
        actual.validate().unwrap();
    }
}
