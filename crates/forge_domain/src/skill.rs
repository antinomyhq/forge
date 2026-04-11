use std::path::PathBuf;

use derive_setters::Setters;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Where a skill was loaded from. Used by higher layers for precedence
/// resolution and for displaying the origin of a skill in listings such as
/// `:plugin list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
#[derive(Default)]
pub enum SkillSource {
    /// Compiled into the Forge binary.
    #[default]
    Builtin,
    /// Contributed by an installed plugin.
    Plugin {
        /// Name of the plugin that owns the skill.
        plugin_name: String,
    },
    /// User-global skill in `~/forge/skills/`.
    GlobalUser,
    /// Skill in the shared `~/.agents/skills/` directory.
    AgentsDir,
    /// Project-local skill in `./.forge/skills/`.
    ProjectCwd,
}

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

    /// Origin of the skill. Defaults to [`SkillSource::Builtin`] for back
    /// compat and is marked `#[serde(default)]` so existing on-disk
    /// `SKILL.md` frontmatter without a `source` field continues to parse.
    #[serde(default)]
    pub source: SkillSource,

    /// Optional extended guidance for when the skill should be invoked.
    /// Mirrors Claude Code's `when_to_use` frontmatter field used by the
    /// auto-activation heuristics. `#[serde(default)]` so older skills
    /// without this field continue to parse.
    #[serde(default)]
    pub when_to_use: Option<String>,

    /// Optional allow-list of tool names that the skill is permitted to
    /// invoke. Mirrors Claude Code's `allowed-tools` frontmatter field.
    /// `None` means the skill inherits the caller's tool permissions.
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,

    /// When `true`, the model itself cannot invoke this skill via a
    /// `skill` tool call; only users may trigger it explicitly. Mirrors
    /// Claude Code's `disable-model-invocation` frontmatter flag.
    #[serde(default)]
    pub disable_model_invocation: bool,

    /// When `true` (the default), users can invoke this skill directly
    /// from the CLI. Mirrors Claude Code's `user-invocable` frontmatter
    /// flag.
    #[serde(default = "default_true")]
    pub user_invocable: bool,
}

/// Serde helper used to default [`Skill::user_invocable`] to `true` so that
/// legacy `SKILL.md` files — which predate the flag — remain invocable by
/// users.
fn default_true() -> bool {
    true
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
            source: SkillSource::default(),
            when_to_use: None,
            allowed_tools: None,
            disable_model_invocation: false,
            user_invocable: true,
        }
    }

    /// Builder-style override for [`Skill::source`]. Kept separate from the
    /// constructor so all existing call sites remain source-compatible.
    pub fn with_source(mut self, source: SkillSource) -> Self {
        self.source = source;
        self
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

    #[test]
    fn test_skill_default_source_is_builtin() {
        // Fixture
        let fixture = Skill::new("s", "p", "d");

        // Assert
        assert_eq!(fixture.source, SkillSource::Builtin);
    }

    #[test]
    fn test_skill_with_source_plugin() {
        // Fixture
        let fixture = Skill::new("s", "p", "d")
            .with_source(SkillSource::Plugin { plugin_name: "demo".into() });

        // Assert
        assert_eq!(
            fixture.source,
            SkillSource::Plugin { plugin_name: "demo".into() }
        );
    }

    #[test]
    fn test_skill_source_serde_roundtrip() {
        let variants = vec![
            SkillSource::Builtin,
            SkillSource::Plugin { plugin_name: "demo".into() },
            SkillSource::GlobalUser,
            SkillSource::AgentsDir,
            SkillSource::ProjectCwd,
        ];

        for original in variants {
            let json = serde_json::to_string(&original).unwrap();
            let roundtrip: SkillSource = serde_json::from_str(&json).unwrap();
            assert_eq!(roundtrip, original, "roundtrip failed for {json}");
        }
    }

    #[test]
    fn test_skill_deserializes_without_source_field() {
        // Old SKILL.md frontmatter -> Skill JSON must still work because
        // `source` is `#[serde(default)]`.
        let json = r#"{
            "name": "legacy",
            "path": null,
            "command": "body",
            "description": "legacy skill",
            "resources": []
        }"#;
        let actual: Skill = serde_json::from_str(json).unwrap();
        assert_eq!(actual.name, "legacy");
        assert_eq!(actual.source, SkillSource::Builtin);
    }

    #[test]
    fn test_skill_new_applies_extended_field_defaults() {
        // `Skill::new` should populate the Claude-Code-aligned extended
        // fields with documented defaults so callers that do not set them
        // explicitly get predictable behaviour.
        let fixture = Skill::new("s", "p", "d");

        assert_eq!(fixture.when_to_use, None);
        assert_eq!(fixture.allowed_tools, None);
        assert!(!fixture.disable_model_invocation);
        assert!(fixture.user_invocable);
    }

    #[test]
    fn test_skill_deserializes_without_extended_fields() {
        // Legacy persisted Skills must continue to parse: the new
        // `when_to_use`, `allowed_tools`, `disable_model_invocation`, and
        // `user_invocable` fields are all `#[serde(default)]` so their
        // absence must not cause a deserialization error.
        let json = r#"{
            "name": "legacy",
            "path": null,
            "command": "body",
            "description": "legacy skill",
            "resources": []
        }"#;
        let actual: Skill = serde_json::from_str(json).unwrap();
        assert_eq!(actual.when_to_use, None);
        assert_eq!(actual.allowed_tools, None);
        assert!(!actual.disable_model_invocation);
        assert!(actual.user_invocable);
    }

    #[test]
    fn test_skill_extended_fields_setters() {
        // `derive_setters::Setters` should expose setters for the new
        // fields so the repository loader can populate them without
        // having to mutate the struct manually.
        let fixture = Skill::new("s", "p", "d")
            .when_to_use("when the user asks")
            .allowed_tools(vec!["read".to_string(), "write".to_string()]);

        assert_eq!(fixture.when_to_use.as_deref(), Some("when the user asks"));
        assert_eq!(
            fixture.allowed_tools.as_deref(),
            Some(["read".to_string(), "write".to_string()].as_slice())
        );
    }
}
