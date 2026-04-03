use std::sync::Arc;

use anyhow::{Context, Result};
use forge_app::SkillSearchService;
use forge_domain::Skill;
use tokio::sync::OnceCell;

/// Service for searching and recommending relevant skills based on a user query.
/// Takes all available local skills, sends them to the forge backend via gRPC,
/// and returns skills ranked by relevance to the task at hand.
pub struct ForgeSkillSearch<R, S> {
    skill_repository: Arc<R>,
    search_repository: Arc<S>,
    cache: OnceCell<Vec<Skill>>,
}

impl<R, S> ForgeSkillSearch<R, S> {
    /// Creates a new skill search service
    ///
    /// # Arguments
    /// * `skill_repository` - Repository for loading available skills
    /// * `search_repository` - Repository for searching/ranking skills via gRPC
    pub fn new(skill_repository: Arc<R>, search_repository: Arc<S>) -> Self {
        Self {
            skill_repository,
            search_repository,
            cache: OnceCell::new(),
        }
    }
}

#[async_trait::async_trait]
impl<R: forge_domain::SkillRepository, S: forge_domain::SkillSearchRepository> SkillSearchService
    for ForgeSkillSearch<R, S>
{
    async fn search_skills(&self, query: String, limit: Option<u32>) -> Result<Vec<Skill>> {
        // Load all available skills from cache or repository
        let all_skills = self.get_or_load_skills().await?;

        // If there are no skills, return early
        if all_skills.is_empty() {
            return Ok(vec![]);
        }

        // Search for relevant skills via the backend
        let ranked_skills = self
            .search_repository
            .search_skills(&query, all_skills.clone(), limit)
            .await
            .context("Failed to search skills via backend")?;

        Ok(ranked_skills)
    }
}

impl<R: forge_domain::SkillRepository, S: forge_domain::SkillSearchRepository> ForgeSkillSearch<R, S> {
    /// Gets skills from cache or loads them from repository if not cached
    async fn get_or_load_skills(&self) -> anyhow::Result<&Vec<Skill>> {
        self.cache
            .get_or_try_init(|| async {
                self.skill_repository
                    .load_skills()
                    .await
                    .context("Failed to load skills")
            })
            .await
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

    struct MockSkillSearchRepository {
        return_skills: Vec<Skill>,
    }

    #[async_trait::async_trait]
    impl forge_domain::SkillSearchRepository for MockSkillSearchRepository {
        async fn search_skills(
            &self,
            _query: &str,
            _skills: Vec<Skill>,
            limit: Option<u32>,
        ) -> Result<Vec<Skill>> {
            let mut result = self.return_skills.clone();
            if let Some(limit) = limit {
                result.truncate(limit as usize);
            }
            Ok(result)
        }
    }

    #[tokio::test]
    async fn test_search_skills_returns_ranked_results() {
        // Fixture
        let expected_skill = Skill::new(
            "semantic-search",
            "Find code semantically",
            "Semantic code search"
        );

        let mock_skill_repo = Arc::new(MockSkillRepository {
            skills: vec![expected_skill.clone()],
        });
        let mock_search_repo = Arc::new(MockSkillSearchRepository {
            return_skills: vec![expected_skill.clone()],
        });

        let service = ForgeSkillSearch::new(mock_skill_repo, mock_search_repo);

        // Actual
        let actual = service.search_skills("code search".to_string(), None).await.unwrap();

        // Expected
        assert_eq!(actual, vec![expected_skill]);
    }

    #[tokio::test]
    async fn test_search_skills_with_limit() {
        // Fixture
        let skill1 = Skill::new("skill-1", "First skill", "First skill desc");
        let skill2 = Skill::new("skill-2", "Second skill", "Second skill desc");
        let skill3 = Skill::new("skill-3", "Third skill", "Third skill desc");

        let mock_skill_repo = Arc::new(MockSkillRepository {
            skills: vec![skill1.clone(), skill2.clone(), skill3.clone()],
        });
        let mock_search_repo = Arc::new(MockSkillSearchRepository {
            return_skills: vec![skill1.clone(), skill2.clone(), skill3.clone()],
        });

        let service = ForgeSkillSearch::new(mock_skill_repo, mock_search_repo);

        // Actual - apply limit of 2
        let actual = service.search_skills("test".to_string(), Some(2)).await.unwrap();

        // Expected - only 2 skills returned
        assert_eq!(actual.len(), 2);
        assert_eq!(actual, vec![skill1, skill2]);
    }

    #[tokio::test]
    async fn test_search_skills_empty_repo() {
        // Fixture - empty skills
        let mock_skill_repo = Arc::new(MockSkillRepository { skills: vec![] });
        let mock_search_repo = Arc::new(MockSkillSearchRepository { return_skills: vec![] });

        let service = ForgeSkillSearch::new(mock_skill_repo, mock_search_repo);

        // Actual
        let actual = service.search_skills("code search".to_string(), None).await.unwrap();

        // Expected - empty result
        assert!(actual.is_empty());
    }
}
