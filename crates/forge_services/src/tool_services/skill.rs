use std::sync::Arc;

use anyhow::{Context, anyhow};
use forge_app::SkillFetchService;
use forge_domain::{
    AuthDetails, ContextEngineRepository, ProviderId, ProviderRepository, SelectedSkill, Skill,
    SkillSelectionParams,
};
use tokio::sync::OnceCell;

/// Loads specialized skills for specific task types. ALWAYS check the
/// available_skills list when a user request matches a skill's description or
/// trigger conditions. Skills provide domain-specific workflows and must be
/// invoked BEFORE attempting the task directly. Only invoke skills listed in
/// available_skills. Do not invoke a skill that is already active.
pub struct ForgeSkillFetch<R> {
    repository: Arc<R>,
    cache: OnceCell<Vec<Skill>>,
}

impl<R> ForgeSkillFetch<R> {
    /// Creates a new skill fetch tool
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository, cache: OnceCell::new() }
    }
}

#[async_trait::async_trait]
impl<R: forge_domain::SkillRepository + ProviderRepository + ContextEngineRepository>
    SkillFetchService for ForgeSkillFetch<R>
{
    async fn fetch_skill(&self, skill_name: String) -> anyhow::Result<Skill> {
        // Load skills from cache or repository
        let skills = self.get_or_load_skills().await?;

        // Find the requested skill
        skills
            .iter()
            .find(|skill| skill.name == skill_name)
            .cloned()
            .ok_or_else(|| {
                anyhow!("Skill '{skill_name}' not found. Please check the available skills list.")
            })
    }

    async fn list_skills(&self) -> anyhow::Result<Vec<Skill>> {
        self.get_or_load_skills().await.cloned()
    }

    async fn recommend_skills(&self, use_case: String) -> anyhow::Result<Vec<SelectedSkill>> {
        // Get auth token and skills in parallel
        let (credential, skills) = tokio::join!(
            self.repository.get_credential(&ProviderId::FORGE_SERVICES),
            self.get_or_load_skills()
        );

        let token = match credential?
            .ok_or(forge_domain::Error::AuthTokenNotFound)?
            .auth_details
        {
            AuthDetails::ApiKey(token) => token,
            _ => anyhow::bail!("ForgeServices credential must be an API key"),
        };

        let skill_infos: Vec<_> = skills?
            .iter()
            .map(|s| forge_domain::SkillInfo::new(&s.name, &s.description))
            .collect();

        // Build params and call the repository
        let params = SkillSelectionParams::new(skill_infos, use_case);
        self.repository
            .select_skill(params, &token)
            .await
            .context("Failed to select skills")
    }
}

impl<R: forge_domain::SkillRepository> ForgeSkillFetch<R> {
    /// Gets skills from cache or loads them from repository if not cached
    async fn get_or_load_skills(&self) -> anyhow::Result<&Vec<Skill>> {
        self.cache
            .get_or_try_init(|| async {
                self.repository
                    .load_skills()
                    .await
                    .context("Failed to load skills")
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    use forge_domain::{
        ApiKey, AuthCredential, CodeSearchQuery, FileDeletion, FileHash, FileUpload,
        FileUploadInfo, Node, Skill, WorkspaceFiles, WorkspaceId, WorkspaceInfo,
    };
    use pretty_assertions::assert_eq;

    use super::*;

    struct MockInfra {
        skills: Vec<Skill>,
    }

    #[async_trait::async_trait]
    impl forge_domain::SkillRepository for MockInfra {
        async fn load_skills(&self) -> anyhow::Result<Vec<Skill>> {
            Ok(self.skills.clone())
        }
    }

    #[async_trait::async_trait]
    impl ProviderRepository for MockInfra {
        async fn get_all_providers(&self) -> anyhow::Result<Vec<forge_domain::AnyProvider>> {
            Ok(vec![])
        }

        async fn get_provider(
            &self,
            _id: ProviderId,
        ) -> anyhow::Result<forge_domain::Provider<url::Url>> {
            unimplemented!()
        }

        async fn upsert_credential(&self, _credential: AuthCredential) -> anyhow::Result<()> {
            Ok(())
        }

        async fn get_credential(&self, id: &ProviderId) -> anyhow::Result<Option<AuthCredential>> {
            if *id == ProviderId::FORGE_SERVICES {
                let mut url_params = HashMap::new();
                url_params.insert(
                    "user_id".to_string().into(),
                    "test_user_id".to_string().into(),
                );

                Ok(Some(AuthCredential {
                    id: ProviderId::FORGE_SERVICES,
                    auth_details: AuthDetails::ApiKey("test_token".to_string().into()),
                    url_params,
                }))
            } else {
                Ok(None)
            }
        }

        async fn remove_credential(&self, _id: &ProviderId) -> anyhow::Result<()> {
            Ok(())
        }

        async fn migrate_env_credentials(
            &self,
        ) -> anyhow::Result<Option<forge_domain::MigrationResult>> {
            Ok(None)
        }
    }

    #[async_trait::async_trait]
    impl ContextEngineRepository for MockInfra {
        async fn authenticate(&self) -> anyhow::Result<forge_domain::WorkspaceAuth> {
            unimplemented!()
        }

        async fn create_workspace(
            &self,
            _: &Path,
            _: &ApiKey,
        ) -> anyhow::Result<forge_domain::WorkspaceId> {
            unimplemented!()
        }

        async fn upload_files(&self, _: &FileUpload, _: &ApiKey) -> anyhow::Result<FileUploadInfo> {
            unimplemented!()
        }

        async fn search(&self, _: &CodeSearchQuery<'_>, _: &ApiKey) -> anyhow::Result<Vec<Node>> {
            unimplemented!()
        }

        async fn list_workspaces(&self, _: &ApiKey) -> anyhow::Result<Vec<WorkspaceInfo>> {
            unimplemented!()
        }

        async fn get_workspace(
            &self,
            _: &WorkspaceId,
            _: &ApiKey,
        ) -> anyhow::Result<Option<WorkspaceInfo>> {
            unimplemented!()
        }

        async fn list_workspace_files(
            &self,
            _: &WorkspaceFiles,
            _: &ApiKey,
        ) -> anyhow::Result<Vec<FileHash>> {
            unimplemented!()
        }

        async fn delete_files(&self, _: &FileDeletion, _: &ApiKey) -> anyhow::Result<()> {
            unimplemented!()
        }

        async fn delete_workspace(&self, _: &WorkspaceId, _: &ApiKey) -> anyhow::Result<()> {
            unimplemented!()
        }

        async fn select_skill(
            &self,
            _: SkillSelectionParams,
            _: &ApiKey,
        ) -> anyhow::Result<Vec<SelectedSkill>> {
            Ok(vec![SelectedSkill::new("test-skill", 0.95, 1)])
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
        let infra = MockInfra { skills: skills.clone() };
        let fetch_service = ForgeSkillFetch::new(Arc::new(infra));

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
        let infra = MockInfra { skills };
        let fetch_service = ForgeSkillFetch::new(Arc::new(infra));

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
        let infra = MockInfra { skills: expected.clone() };
        let fetch_service = ForgeSkillFetch::new(Arc::new(infra));

        // Act
        let actual = fetch_service.list_skills().await.unwrap();

        // Assert
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_recommend_skill() {
        // Fixture
        let skills = vec![
            Skill::new("pdf", "Handle PDF files", "PDF handling skill").path("/skills/pdf.md"),
        ];
        let infra = MockInfra { skills };
        let fetch_service = ForgeSkillFetch::new(Arc::new(infra));

        // Act
        let actual = fetch_service
            .recommend_skills("I need to handle PDF files".to_string())
            .await
            .unwrap();

        // Assert
        let expected = vec![SelectedSkill::new("test-skill", 0.95, 1)];
        assert_eq!(actual, expected);
    }
}
