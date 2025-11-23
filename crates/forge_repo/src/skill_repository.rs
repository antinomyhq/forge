use std::sync::Arc;

use anyhow::Context;
use forge_app::domain::Skill;
use forge_app::{DirectoryReaderInfra, EnvironmentInfra, FileInfoInfra, FileReaderInfra};
use forge_domain::SkillRepository;
use futures::future::join_all;
use gray_matter::engine::YAML;
use gray_matter::Matter;
use serde::Deserialize;

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
/// - **Global skills**: `{HOME}/.forge/skills/<skill-name>/SKILL.md`
/// - **CWD skills**: `./.forge/skills/<skill-name>/SKILL.md` (relative to
///   current working directory)
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

    /// Loads built-in skills that are embedded in the application
    fn load_builtin_skills(&self) -> Vec<Skill> {
        let builtin_skills = vec![
            (
                "forge://skills/skill-creation/SKILL.md",
                include_str!("skills/skill-creation.md"),
            ),
            (
                "forge://skills/plan-executor/SKILL.md",
                include_str!("skills/plan-executor.md"),
            ),
        ];

        builtin_skills
            .into_iter()
            .filter_map(|(path, content)| extract_skill(path, content))
            .collect()
    }
}

#[async_trait::async_trait]
impl<I: FileInfoInfra + EnvironmentInfra + DirectoryReaderInfra + FileReaderInfra> SkillRepository
    for ForgeSkillRepository<I>
{
    /// Loads all available skills from the skills directory
    ///
    /// # Errors
    /// Returns an error if skill loading fails
    async fn load_skills(&self) -> anyhow::Result<Vec<Skill>> {
        let mut skills = Vec::new();
        let env = self.infra.get_environment();

        // Load built-in skills
        let builtin_skills = self.load_builtin_skills();
        skills.extend(builtin_skills);

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

        // Resolve conflicts by keeping the last occurrence (CWD > Global > Built-in)
        Ok(resolve_skill_conflicts(skills))
    }
}

impl<I: FileInfoInfra + EnvironmentInfra + DirectoryReaderInfra + FileReaderInfra>
    ForgeSkillRepository<I>
{
    /// Loads skills from a specific directory by listing subdirectories first,
    /// then reading SKILL.md from each subdirectory if it exists
    async fn load_skills_from_dir(&self, dir: &std::path::Path) -> anyhow::Result<Vec<Skill>> {
        if !self.infra.exists(dir).await? {
            return Ok(vec![]);
        }

        // List all entries in the directory
        let entries = self
            .infra
            .list_directory_entries(dir)
            .await
            .with_context(|| format!("Failed to list directory: {}", dir.display()))?;

        // Filter for directories only
        let subdirs: Vec<_> = entries
            .into_iter()
            .filter_map(|(path, is_dir)| if is_dir { Some(path) } else { None })
            .collect();

        // Read SKILL.md from each subdirectory in parallel
        let futures = subdirs.into_iter().map(|subdir| {
            let infra = Arc::clone(&self.infra);
            async move {
                let skill_path = subdir.join("SKILL.md");

                // Check if SKILL.md exists in this subdirectory
                if infra.exists(&skill_path).await? {
                    // Read the file content
                    match infra.read_utf8(&skill_path).await {
                        Ok(content) => {
                            let path_str = skill_path.display().to_string();
                            let skill_name = subdir
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or("unknown")
                                .to_string();

                            // Try to extract skill from front matter, otherwise create with
                            // directory name
                            if let Some(skill) = extract_skill(&path_str, &content) {
                                Ok(Some(skill))
                            } else {
                                // Fallback: create skill with directory name if front matter is
                                // missing
                                Ok(Some(Skill::new(
                                    skill_name,
                                    path_str,
                                    content,
                                    String::new(),
                                )))
                            }
                        }
                        Err(e) => {
                            // Log warning but continue processing other skills
                            tracing::warn!(
                                "Failed to read skill file {}: {}",
                                skill_path.display(),
                                e
                            );
                            Ok(None)
                        }
                    }
                } else {
                    Ok(None)
                }
            }
        });

        // Execute all futures in parallel and collect results
        let results = join_all(futures).await;
        let skills: Vec<Skill> = results
            .into_iter()
            .filter_map(|result: anyhow::Result<Option<Skill>>| result.ok().flatten())
            .collect();

        Ok(skills)
    }
}

/// Private type for parsing skill YAML front matter
#[derive(Debug, Deserialize)]
struct SkillMetadata {
    /// Optional name of the skill (overrides filename if present)
    name: Option<String>,
    /// Optional description of the skill
    description: Option<String>,
}

/// Extracts metadata from the skill markdown content using YAML front matter
///
/// Parses YAML front matter from the markdown content and extracts skill
/// metadata. Expected format:
/// ```markdown
/// ---
/// name: "skill-name"
/// description: "Your description here"
/// ---
/// # Skill content...
/// ```
///
/// Returns a tuple of (name, description) where both are Option<String>.
fn extract_skill(path: &str, content: &str) -> Option<Skill> {
    let matter = Matter::<YAML>::new();
    let result = matter.parse::<SkillMetadata>(content);
    let path = path.into();
    result.ok().and_then(|parsed| {
        let command = parsed.content;
        parsed
            .data
            .and_then(|data| data.name.zip(data.description))
            .map(|(name, description)| Skill { name, path, command, description })
    })
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
            Skill::new("skill1", "/path/skill1.md", "prompt1", "desc1"),
            Skill::new("skill2", "/path/skill2.md", "prompt2", "desc2"),
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
            Skill::new(
                "skill1",
                "/global/skill1.md",
                "global prompt",
                "global desc",
            ),
            Skill::new("skill2", "/global/skill2.md", "prompt2", "desc2"),
            Skill::new("skill1", "/cwd/skill1.md", "cwd prompt", "cwd desc"),
        ];

        // Act
        let actual = resolve_skill_conflicts(skills);

        // Assert
        assert_eq!(actual.len(), 2);
        assert_eq!(actual[0].name, "skill1");
        assert_eq!(actual[0].path, std::path::Path::new("/cwd/skill1.md"));
        assert_eq!(actual[0].command, "cwd prompt");
        assert_eq!(actual[1].name, "skill2");
    }

    #[test]
    fn test_load_builtin_skills() {
        // Fixture
        let repo = ForgeSkillRepository { infra: Arc::new(()) };

        // Act
        let actual = repo.load_builtin_skills();

        // Assert
        assert_eq!(actual.len(), 2);

        // Check skill-creator
        let skill_creator = actual.iter().find(|s| s.name == "skill-creator").unwrap();
        assert_eq!(
            skill_creator.path,
            std::path::Path::new("forge://skills/skill-creation/SKILL.md")
        );
        assert_eq!(
            skill_creator.description,
            "Guide for creating effective skills. This skill should be used when users want to create a new skill (or update an existing skill) that extends your capabilities with specialized knowledge, workflows, or tool integrations."
        );
        assert!(skill_creator.command.contains("Skill Creator"));
        assert!(skill_creator.command.contains("creating effective skills"));

        // Check plan-executor
        let plan_executor = actual.iter().find(|s| s.name == "plan-executor").unwrap();
        assert_eq!(
            plan_executor.path,
            std::path::Path::new("forge://skills/plan-executor/SKILL.md")
        );
        assert!(plan_executor
            .description
            .contains("Execute structured task plans"));
        assert!(plan_executor.command.contains("Plan Executor"));
    }

    #[test]
    fn test_extract_metadata_with_name_and_description() {
        // Fixture
        let path = "fixtures/skills/with_name_and_description.md";
        let content = include_str!("fixtures/skills/with_name_and_description.md");

        // Act
        let actual = extract_skill(path, content);

        // Assert
        let expected = Some(Skill::new(
            "pdf-handler",
            path,
            "# PDF Handler\n\nContent here...",
            "This is a skill for handling PDF files",
        ));
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_extract_metadata_with_name_only() {
        // Fixture
        let path = "fixtures/skills/with_name_only.md";
        let content = include_str!("fixtures/skills/with_name_only.md");

        // Act
        let actual = extract_skill(path, content);

        // Assert - Returns None because description is missing
        assert_eq!(actual, None);
    }

    #[test]
    fn test_extract_metadata_with_description_only() {
        // Fixture
        let path = "fixtures/skills/with_description_only.md";
        let content = include_str!("fixtures/skills/with_description_only.md");

        // Act
        let actual = extract_skill(path, content);

        // Assert - Returns None because name is missing
        assert_eq!(actual, None);
    }

    #[test]
    fn test_extract_metadata_no_front_matter() {
        // Fixture
        let path = "fixtures/skills/no_front_matter.md";
        let content = include_str!("fixtures/skills/no_front_matter.md");

        // Act
        let actual = extract_skill(path, content);

        // Assert - Returns None because front matter is missing
        assert_eq!(actual, None);
    }
}
