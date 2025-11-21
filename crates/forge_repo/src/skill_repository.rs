use std::sync::Arc;

use anyhow::Context;
use forge_app::domain::Skill;
use forge_app::{DirectoryReaderInfra, EnvironmentInfra, FileInfoInfra};
use forge_domain::SkillRepository;

/// Repository implementation for loading skills from multiple sources:
/// 1. Built-in skills (embedded in the application)
/// 2. Global custom skills (from ~/.forge/skills/ directory)
/// 3. Project-local skills (from .forge/skills/ directory in current working
///    directory)
///
/// ## Skill Precedence
/// When skills have duplicate names across different sources, the precedence
/// order is: **CWD (project-local) > Global custom > Built-in**
///
/// This means project-local skills can override global skills, and both can
/// override built-in skills.
///
/// ## Directory Resolution
/// - **Built-in skills**: Embedded in application binary
/// - **Global skills**: `{HOME}/.forge/skills/*.md`
/// - **CWD skills**: `./.forge/skills/*.md` (relative to current working
///   directory)
///
/// Missing directories are handled gracefully and don't prevent loading from
/// other sources.
pub struct ForgeSkillRepository<I> {
    infra: Arc<I>,
}

impl<I> ForgeSkillRepository<I> {
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }
}

#[async_trait::async_trait]
impl<I: FileInfoInfra + EnvironmentInfra + DirectoryReaderInfra> SkillRepository
    for ForgeSkillRepository<I>
{
    /// Loads all available skills from the skills directory
    ///
    /// # Errors
    /// Returns an error if skill loading fails
    async fn load_skills(&self) -> anyhow::Result<Vec<Skill>> {
        let mut skills = Vec::new();
        let env = self.infra.get_environment();

        // Load global skills
        if let Some(home) = &env.home {
            let global_dir = home.join(".forge/skills");
            let global_skills = self.load_skills_from_dir(&global_dir).await?;
            skills.extend(global_skills);
        }

        // Load project-local skills
        let cwd_dir = env.cwd.join(".forge/skills");
        let cwd_skills = self.load_skills_from_dir(&cwd_dir).await?;
        skills.extend(cwd_skills);

        // Resolve conflicts by keeping the last occurrence (CWD > Global)
        Ok(resolve_skill_conflicts(skills))
    }
}

impl<I: FileInfoInfra + EnvironmentInfra + DirectoryReaderInfra> ForgeSkillRepository<I> {
    /// Loads skills from a specific directory
    async fn load_skills_from_dir(&self, dir: &std::path::Path) -> anyhow::Result<Vec<Skill>> {
        if !self.infra.exists(dir).await? {
            return Ok(vec![]);
        }

        // Read all .md files in the directory
        let files = self
            .infra
            .read_directory_files(dir, Some("*.md"))
            .await
            .with_context(|| format!("Failed to read skills from: {}", dir.display()))?;

        let skills: Vec<Skill> = files
            .into_iter()
            .map(|(path, content)| {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                Skill::new(name, path.display().to_string(), content)
            })
            .collect();

        Ok(skills)
    }
}

/// Resolves skill conflicts by keeping the last occurrence of each skill name
///
/// This gives precedence to later sources (CWD > Global)
fn resolve_skill_conflicts(skills: Vec<Skill>) -> Vec<Skill> {
    let mut seen = std::collections::HashMap::new();
    let mut result = Vec::new();

    for skill in skills {
        if let Some(idx) = seen.get(&skill.name) {
            // Replace the earlier skill with the same name
            result[*idx] = skill.clone();
        } else {
            // First occurrence of this skill name
            seen.insert(skill.name.clone(), result.len());
            result.push(skill);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_resolve_skill_conflicts_no_duplicates() {
        // Fixture
        let skills = vec![
            Skill::new("skill1", "/path/skill1.md", "prompt1"),
            Skill::new("skill2", "/path/skill2.md", "prompt2"),
        ];

        // Act
        let actual = resolve_skill_conflicts(skills.clone());

        // Assert
        let expected = skills;
        assert_eq!(actual.len(), expected.len());
        assert_eq!(actual[0].name, expected[0].name);
        assert_eq!(actual[1].name, expected[1].name);
    }

    #[test]
    fn test_resolve_skill_conflicts_with_duplicates() {
        // Fixture
        let skills = vec![
            Skill::new("skill1", "/global/skill1.md", "global prompt"),
            Skill::new("skill2", "/global/skill2.md", "prompt2"),
            Skill::new("skill1", "/cwd/skill1.md", "cwd prompt"),
        ];

        // Act
        let actual = resolve_skill_conflicts(skills);

        // Assert
        assert_eq!(actual.len(), 2);
        assert_eq!(actual[0].name, "skill1");
        assert_eq!(actual[0].path, "/cwd/skill1.md");
        assert_eq!(actual[0].prompt, "cwd prompt");
        assert_eq!(actual[1].name, "skill2");
    }
}
