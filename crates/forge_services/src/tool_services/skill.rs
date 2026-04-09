use std::sync::Arc;

use anyhow::{Context, anyhow};
use forge_app::SkillFetchService;
use forge_domain::Skill;
use tokio::sync::RwLock;

/// Loads specialized skills for specific task types. ALWAYS check the
/// available_skills list when a user request matches a skill's description or
/// trigger conditions. Skills provide domain-specific workflows and must be
/// invoked BEFORE attempting the task directly. Only invoke skills listed in
/// available_skills. Do not invoke a skill that is already active.
pub struct ForgeSkillFetch<R> {
    repository: Arc<R>,
    /// In-memory cache of skills loaded from the repository.
    ///
    /// Uses an `RwLock<Option<_>>` rather than [`tokio::sync::OnceCell`] so
    /// that [`invalidate_cache`](SkillFetchService::invalidate_cache) can
    /// reset the cache and force a reload — this is required for
    /// mid-session skill discovery (e.g. after the `create-skill` workflow
    /// writes a new `SKILL.md` to disk).
    cache: RwLock<Option<Vec<Skill>>>,
}

impl<R> ForgeSkillFetch<R> {
    /// Creates a new skill fetch tool
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository, cache: RwLock::new(None) }
    }
}

#[async_trait::async_trait]
impl<R: forge_domain::SkillRepository> SkillFetchService for ForgeSkillFetch<R> {
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
        self.get_or_load_skills().await
    }

    async fn invalidate_cache(&self) {
        let mut guard = self.cache.write().await;
        *guard = None;
    }
}

impl<R: forge_domain::SkillRepository> ForgeSkillFetch<R> {
    /// Gets skills from cache or loads them from repository if not cached.
    ///
    /// Uses a double-checked locking pattern: a read lock is taken first (the
    /// fast path for already-cached data); only if the cache is empty do we
    /// upgrade to a write lock and hit the repository.
    async fn get_or_load_skills(&self) -> anyhow::Result<Vec<Skill>> {
        // Fast path: read lock, return cached data if present.
        {
            let guard = self.cache.read().await;
            if let Some(skills) = guard.as_ref() {
                return Ok(skills.clone());
            }
        }

        // Slow path: acquire write lock and repopulate.
        let mut guard = self.cache.write().await;
        // Re-check under the write lock in case another task populated it
        // between our read and write acquisitions.
        if let Some(skills) = guard.as_ref() {
            return Ok(skills.clone());
        }

        let skills = self
            .repository
            .load_skills()
            .await
            .context("Failed to load skills")?;
        *guard = Some(skills.clone());
        Ok(skills)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use forge_domain::Skill;
    use pretty_assertions::assert_eq;
    use tokio::sync::Mutex as AsyncMutex;

    use super::*;

    /// Mock repository that supports mid-test mutation so we can verify
    /// cache behaviour and invalidation semantics.
    struct MockSkillRepository {
        skills: AsyncMutex<Vec<Skill>>,
        load_count: AtomicUsize,
    }

    impl MockSkillRepository {
        fn new(skills: Vec<Skill>) -> Self {
            Self {
                skills: AsyncMutex::new(skills),
                load_count: AtomicUsize::new(0),
            }
        }

        async fn set_skills(&self, skills: Vec<Skill>) {
            *self.skills.lock().await = skills;
        }

        fn load_count(&self) -> usize {
            self.load_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl forge_domain::SkillRepository for MockSkillRepository {
        async fn load_skills(&self) -> anyhow::Result<Vec<Skill>> {
            self.load_count.fetch_add(1, Ordering::SeqCst);
            Ok(self.skills.lock().await.clone())
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
        let repo = MockSkillRepository::new(skills.clone());
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
        let repo = MockSkillRepository::new(skills);
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
        let repo = MockSkillRepository::new(expected.clone());
        let fetch_service = ForgeSkillFetch::new(Arc::new(repo));

        // Act
        let actual = fetch_service.list_skills().await.unwrap();

        // Assert
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_list_skills_caches_across_calls() {
        // Fixture: repository should only be hit once across multiple list calls.
        let repo = Arc::new(MockSkillRepository::new(vec![
            Skill::new("pdf", "Handle PDF files", "PDF handling skill").path("/skills/pdf.md"),
        ]));
        let fetch_service = ForgeSkillFetch::new(repo.clone());

        // Act
        let _ = fetch_service.list_skills().await.unwrap();
        let _ = fetch_service.list_skills().await.unwrap();
        let _ = fetch_service.list_skills().await.unwrap();

        // Assert
        assert_eq!(repo.load_count(), 1);
    }

    #[tokio::test]
    async fn test_invalidate_cache_forces_reload() {
        // Fixture
        let repo = Arc::new(MockSkillRepository::new(vec![
            Skill::new("pdf", "Handle PDF files", "PDF handling skill").path("/skills/pdf.md"),
        ]));
        let fetch_service = ForgeSkillFetch::new(repo.clone());

        // Prime the cache.
        let first = fetch_service.list_skills().await.unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(repo.load_count(), 1);

        // Mutate the repository under the hood (e.g. create-skill writes a new
        // SKILL.md on disk) and invalidate the cache.
        repo.set_skills(vec![
            Skill::new("pdf", "Handle PDF files", "PDF handling skill").path("/skills/pdf.md"),
            Skill::new("new", "Brand new skill", "Newly created").path("/skills/new.md"),
        ])
        .await;
        fetch_service.invalidate_cache().await;

        // Act: next list call should see the new skill.
        let second = fetch_service.list_skills().await.unwrap();

        // Assert
        assert_eq!(second.len(), 2);
        assert!(second.iter().any(|s| s.name == "new"));
        // Exactly one additional repository hit.
        assert_eq!(repo.load_count(), 2);
    }

    #[tokio::test]
    async fn test_invalidate_without_prior_load_is_noop() {
        // Fixture
        let repo = Arc::new(MockSkillRepository::new(vec![]));
        let fetch_service = ForgeSkillFetch::new(repo.clone());

        // Act: invalidating an empty cache must not panic or touch the repo.
        fetch_service.invalidate_cache().await;

        // Assert: repository still untouched.
        assert_eq!(repo.load_count(), 0);
    }
}
