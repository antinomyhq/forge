// Context level enum for GCC
use serde::{Deserialize, Serialize};

/// Represents the level of context being accessed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContextLevel {
    /// The top-level project context.
    Project,
    /// A specific branch.
    Branch(String),
    /// A specific commit within a branch.
    Commit(String),
}

impl ContextLevel {
    /// Helper to create a Project level.
    pub fn project() -> Self {
        Self::Project
    }
    /// Helper to create a branch level.
    pub fn branch(name: impl Into<String>) -> Self {
        Self::Branch(name.into())
    }
    /// Helper to create a commit level.
    pub fn commit(id: impl Into<String>) -> Self {
        Self::Commit(id.into())
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn fixture_project() -> ContextLevel {
        ContextLevel::project()
    }

    fn fixture_branch() -> ContextLevel {
        ContextLevel::branch("feature-xyz")
    }

    fn fixture_commit() -> ContextLevel {
        ContextLevel::commit("c123")
    }

    #[test]
    fn test_context_level_variants() {
        assert_eq!(fixture_project(), ContextLevel::Project);
        assert_eq!(
            fixture_branch(),
            ContextLevel::Branch("feature-xyz".to_string())
        );
        assert_eq!(fixture_commit(), ContextLevel::Commit("c123".to_string()));
    }
}
