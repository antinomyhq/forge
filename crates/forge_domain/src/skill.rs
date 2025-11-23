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
    pub path: PathBuf,

    /// Content/prompt loaded from the markdown file
    pub command: String,

    /// Description of the skill
    pub description: String,
}

impl Skill {
    /// Creates a new Skill with required fields
    ///
    /// # Arguments
    ///
    /// * `name` - The name identifier for the skill
    /// * `path` - The file system path to the skill markdown file
    /// * `prompt` - The skill prompt content
    /// * `description` - A brief description of the skill
    pub fn new(
        name: impl Into<String>,
        path: impl Into<PathBuf>,
        prompt: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            command: prompt.into(),
            description: description.into(),
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
            "/skills/code_review.md",
            "Review this code",
            "A skill for reviewing code quality",
        );

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
            "/skills/code_review.md".into(),
            "Review this code".to_string(),
            "A skill for reviewing code quality".to_string(),
        );
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_skill_with_setters() {
        // Fixture
        let fixture = Skill::new("test", "/path", "prompt", "desc")
            .name("updated_name")
            .path("/updated/path")
            .command("updated prompt")
            .description("updated description");

        // Act
        let actual = fixture;

        // Assert
        let expected = Skill::new(
            "updated_name",
            "/updated/path",
            "updated prompt",
            "updated description",
        );
        assert_eq!(actual, expected);
    }
}
