// commit.rs
use derive_setters::Setters;
use serde::{Deserialize, Serialize};

use crate::gcc::{GccError, GccResult};

/// Represents a commit (milestone) in the GCC context hierarchy.
#[derive(Debug, Clone, Serialize, Deserialize, Setters, PartialEq, Eq, Default)]
#[setters(strip_option, into)]
pub struct Commit {
    /// Unique identifier for the commit.
    pub id: String,
    /// Optional parent commit identifier.
    pub parent: Option<String>,
    /// Name of the branch this commit belongs to.
    pub branch: String,
    /// Optional description of the commit.
    pub description: Option<String>,
    /// Optional timestamp (ISO 8601) when the commit was created.
    pub timestamp: Option<String>,
}

impl Commit {
    /// Creates a new commit with the given id and branch.
    pub fn new(id: impl Into<String>, branch: impl Into<String>) -> Self {
        Self { id: id.into(), branch: branch.into(), ..Default::default() }
    }

    /// Validates that required fields are present.
    pub fn validate(&self) -> GccResult<()> {
        if self.id.trim().is_empty() {
            return Err(GccError::InvalidOperation(
                "Commit id cannot be empty".into(),
            ));
        }
        if self.branch.trim().is_empty() {
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

    fn fixture_commit() -> Commit {
        Commit::new("c123", "main")
            .parent("c122")
            .description("Initial commit")
            .timestamp("2025-08-06T00:00:00Z")
    }

    #[test]
    fn test_commit_creation_and_validation() {
        let actual = fixture_commit();
        let expected = {
            let mut c = Commit::new("c123", "main");
            c = c.parent("c122");
            c = c.description("Initial commit");
            c = c.timestamp("2025-08-06T00:00:00Z");
            c
        };
        assert_eq!(actual, expected);
        actual.validate().unwrap();
    }
}
