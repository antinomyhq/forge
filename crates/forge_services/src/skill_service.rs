use std::sync::Arc;

use forge_app::domain::Skill;
use forge_domain::SkillRepository;

/// Service for loading and managing skills
///
/// This service provides business logic for working with skills, including
/// loading them from the repository and preparing them for use in system
/// prompts.
pub struct SkillService<R> {
    repository: Arc<R>,
}

impl<R> SkillService<R> {
    /// Creates a new SkillService
    ///
    /// # Arguments
    /// * `repository` - The skill repository implementation
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }
}

impl<R: SkillRepository> SkillService<R> {
    /// Loads all available skills
    ///
    /// # Errors
    /// Returns an error if skill loading fails
    pub async fn load_skills(&self) -> anyhow::Result<Vec<Skill>> {
        self.repository.load_skills().await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use pretty_assertions::assert_eq;

    use super::*;

    struct MockSkillRepository {
        skills: Vec<Skill>,
    }

    #[async_trait::async_trait]
    impl SkillRepository for MockSkillRepository {
        async fn load_skills(&self) -> anyhow::Result<Vec<Skill>> {
            Ok(self.skills.clone())
        }
    }

    #[tokio::test]
    async fn test_load_skills_empty() {
        // Fixture
        let repo = Arc::new(MockSkillRepository { skills: vec![] });
        let service = SkillService::new(repo);

        // Act
        let actual = service.load_skills().await.unwrap();

        // Assert
        let expected: Vec<Skill> = vec![];
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_load_skills_with_data() {
        // Fixture
        let skills = vec![
            Skill::new("code_review", "/skills/code_review.md", "Review code"),
            Skill::new("testing", "/skills/testing.md", "Write tests"),
        ];
        let repo = Arc::new(MockSkillRepository { skills: skills.clone() });
        let service = SkillService::new(repo);

        // Act
        let actual = service.load_skills().await.unwrap();

        // Assert
        assert_eq!(actual.len(), 2);
        assert_eq!(actual[0].name, "code_review");
        assert_eq!(actual[1].name, "testing");
    }
}
