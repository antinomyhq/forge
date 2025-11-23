use std::sync::Arc;

use anyhow::{Context, anyhow};
use forge_app::SkillFetchService;
use forge_domain::Skill;

/// Fetches detailed information about a specific skill. Use this tool to load
/// skill content and instructions when you need to understand how to perform a
/// specialized task. Skills provide domain-specific knowledge, workflows, and
/// best practices. Only invoke skills that are listed in the available skills
/// section. Do not invoke a skill that is already active.
pub struct ForgeSkillFetch<R>(Arc<R>);

impl<R> ForgeSkillFetch<R> {
    /// Creates a new skill fetch tool
    pub fn new(repository: Arc<R>) -> Self {
        Self(repository)
    }
}

#[async_trait::async_trait]
impl<R: forge_domain::SkillRepository> SkillFetchService for ForgeSkillFetch<R> {
    async fn fetch_skill(&self, skill_name: String) -> anyhow::Result<Skill> {
        // Load all skills
        let skills = self
            .0
            .load_skills()
            .await
            .context("Failed to load skills")?;

        // Find the requested skill
        skills
            .into_iter()
            .find(|skill| skill.name == skill_name)
            .ok_or_else(|| {
                anyhow!(
                    "Skill '{}' not found. Please check the available skills list.",
                    skill_name
                )
            })
    }

    async fn list_skills(&self) -> anyhow::Result<Vec<Skill>> {
        self.0.load_skills().await.context("Failed to load skills")
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::Skill;
    use pretty_assertions::assert_eq;

    use super::*;

    struct MockSkillRepository {
        skills: Vec<Skill>,
    }

    #[async_trait::async_trait]
    impl forge_domain::SkillRepository for MockSkillRepository {
        async fn load_skills(&self) -> anyhow::Result<Vec<Skill>> {
            Ok(self.skills.clone())
        }
    }

    #[tokio::test]
    async fn test_fetch_skill_found() {
        // Fixture
        let skills = vec![
            Skill::new("pdf", "Handle PDF files", "PDF handling skill").path("/skills/pdf.md"),
            Skill::new("xlsx", "Handle Excel files", "Excel handling skill")
                .path("/skills/xlsx.md"),
        ];
        let repo = MockSkillRepository { skills: skills.clone() };
        let fetch_service = ForgeSkillFetch::new(Arc::new(repo));

        // Act
        let actual = fetch_service.fetch_skill("pdf".to_string()).await;

        // Assert
        assert!(actual.is_ok());
        let expected =
            Skill::new("pdf", "Handle PDF files", "PDF handling skill").path("/skills/pdf.md");
        assert_eq!(actual.unwrap(), expected);
    }

    #[tokio::test]
    async fn test_fetch_skill_not_found() {
        // Fixture
        let skills = vec![
            Skill::new("pdf", "Handle PDF files", "PDF handling skill").path("/skills/pdf.md"),
        ];
        let repo = MockSkillRepository { skills };
        let fetch_service = ForgeSkillFetch::new(Arc::new(repo));

        // Act
        let actual = fetch_service.fetch_skill("unknown".to_string()).await;

        // Assert
        assert!(actual.is_err());
        let error = actual.unwrap_err().to_string();
        assert!(error.contains("Skill 'unknown' not found"));
    }

    #[tokio::test]
    async fn test_list_skills() {
        // Fixture
        let expected = vec![
            Skill::new("pdf", "Handle PDF files", "PDF handling skill").path("/skills/pdf.md"),
            Skill::new("xlsx", "Handle Excel files", "Excel handling skill")
                .path("/skills/xlsx.md"),
        ];
        let repo = MockSkillRepository { skills: expected.clone() };
        let fetch_service = ForgeSkillFetch::new(Arc::new(repo));

        // Act
        let actual = fetch_service.list_skills().await.unwrap();

        // Assert
        assert_eq!(actual, expected);
    }
}
