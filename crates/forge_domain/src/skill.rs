use std::path::PathBuf;

use derive_setters::Setters;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Represents a reusable skill with a name, file path, and prompt content
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Setters, JsonSchema)]
#[setters(strip_option, into)]
pub struct Skill {
    /// Name of the skill
    pub name: String,

    /// File path to the skill markdown file
    pub path: Option<PathBuf>,

    /// Content/prompt loaded from the markdown file
    pub command: String,

    /// Description of the skill
    pub description: String,

    /// List of resource files in the skill directory
    pub resources: Vec<PathBuf>,
}

impl Skill {
    /// Creates a new Skill with required fields
    ///
    /// # Arguments
    ///
    /// * `name` - The name identifier for the skill
    /// * `prompt` - The skill prompt content
    /// * `description` - A brief description of the skill
    pub fn new(
        name: impl Into<String>,
        prompt: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            path: None,
            command: prompt.into(),
            description: description.into(),
            resources: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_skill_creation() {
        // Fixture
        let fixture = Skill::new(
            "code_review",
            "Review this code",
            "A skill for reviewing code quality",
        )
        .path("/skills/code_review.md");

        // Act
        let actual = (
            fixture.name.clone(),
            fixture.path.clone(),
            fixture.command.clone(),
            fixture.description.clone(),
        );

        // Assert
        let expected = (
            "code_review".to_string(),
            Some("/skills/code_review.md".into()),
            "Review this code".to_string(),
            "A skill for reviewing code quality".to_string(),
        );
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_skill_with_setters() {
        // Fixture
        let fixture = Skill::new("test", "prompt", "desc")
            .path("/path")
            .name("updated_name")
            .path("/updated/path")
            .command("updated prompt")
            .description("updated description");

        // Act
        let actual = fixture;

        // Assert
        let expected = Skill::new("updated_name", "updated prompt", "updated description")
            .path("/updated/path");
        assert_eq!(actual, expected);
    }
}
