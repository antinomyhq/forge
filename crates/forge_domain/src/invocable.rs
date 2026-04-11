//! Unified view of invocable commands — the type the LLM sees in the per-turn
//! `<system_reminder>` catalog.
//!
//! In Claude Code, skills (loaded from `skills/*/SKILL.md`) and commands
//! (loaded from `commands/*.md`) flow through the same pipeline and appear as
//! entries in a single `<system-reminder>` listing, differentiated only by
//! whether they were loaded from a `skills/` or a `commands/` directory. Forge
//! mirrors this: [`InvocableCommand`] is the domain-level representation of a
//! single entry in that unified listing, regardless of whether it originated
//! as a [`Skill`](crate::Skill) or a [`Command`](crate::Command).
//!
//! The type is intentionally lightweight: only the fields required to render
//! the listing and to enforce Claude-Code-aligned flags (such as
//! `disable-model-invocation`) are carried. Consumers that need the full body
//! of a skill still call `skill_fetch` (which goes through the
//! [`SkillFetchService`](crate::SkillRepository) cache).

use crate::{Command, CommandSource, Skill, SkillSource};

/// Unified invocable command view that merges plugin/built-in skills and
/// commands.
///
/// Skills and commands flow through the same pipeline in Claude Code; Forge
/// mirrors that by exposing a single listing to the LLM via
/// `<system_reminder>`. See
/// `claude-code/src/utils/plugins/loadPluginCommands.ts:218-412` for the
/// upstream `createPluginCommand()` helper that inspired this shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocableCommand {
    /// Fully-qualified name as it should be passed to `skill_fetch` (for
    /// skills) or invoked as a slash command (for commands). Plugin entries
    /// are already namespaced as `{plugin_name}:{local_name}` by the
    /// repository loaders.
    pub name: String,
    /// Single-line description shown to the LLM in the catalog.
    pub description: String,
    /// Optional extended guidance describing when the entry should be
    /// invoked. Only skills currently carry this; commands default to
    /// `None`.
    pub when_to_use: Option<String>,
    /// Whether the entry was loaded as a skill or as a command.
    pub kind: InvocableKind,
    /// Where the entry was loaded from (built-in, plugin, user, project).
    pub source: InvocableSource,
    /// Mirrors Claude Code's `disable-model-invocation` flag. When `true`,
    /// the entry must be hidden from the LLM's `<system_reminder>` catalog
    /// and refused by `skill_fetch` — users can still invoke it manually.
    /// Commands default to `false` (always model-invocable).
    pub disable_model_invocation: bool,
    /// Mirrors Claude Code's `user-invocable` flag. When `true`, users can
    /// invoke the entry via a slash command. Commands default to `true`
    /// (commands are user-invocable by definition).
    pub user_invocable: bool,
}

/// Discriminator for [`InvocableCommand`] that records the on-disk loading
/// convention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvocableKind {
    /// Loaded from a `skills/` directory (`SKILL.md` + optional resources).
    Skill,
    /// Loaded from a `commands/` directory (standalone `.md` file).
    Command,
}

/// Provenance of an [`InvocableCommand`]. Collapses the separate
/// [`SkillSource`] and [`CommandSource`] enums into a single unified
/// vocabulary so that the LLM-facing listing does not leak the internal
/// skill-vs-command distinction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvocableSource {
    /// Compiled into the Forge binary.
    Builtin,
    /// Contributed by an installed plugin.
    Plugin {
        /// Name of the plugin that owns the entry.
        plugin_name: String,
    },
    /// User-scoped entry (global `~/forge/...` or `~/.agents/...`).
    User,
    /// Project-local entry (`./.forge/...`).
    Project,
}

impl From<&Skill> for InvocableCommand {
    fn from(skill: &Skill) -> Self {
        let source = match &skill.source {
            SkillSource::Builtin => InvocableSource::Builtin,
            SkillSource::Plugin { plugin_name } => {
                InvocableSource::Plugin { plugin_name: plugin_name.clone() }
            }
            SkillSource::GlobalUser | SkillSource::AgentsDir => InvocableSource::User,
            SkillSource::ProjectCwd => InvocableSource::Project,
        };

        Self {
            name: skill.name.clone(),
            description: skill.description.clone(),
            when_to_use: skill.when_to_use.clone(),
            kind: InvocableKind::Skill,
            source,
            disable_model_invocation: skill.disable_model_invocation,
            user_invocable: skill.user_invocable,
        }
    }
}

impl From<&Command> for InvocableCommand {
    fn from(command: &Command) -> Self {
        let source = match &command.source {
            CommandSource::Builtin => InvocableSource::Builtin,
            CommandSource::Plugin { plugin_name } => {
                InvocableSource::Plugin { plugin_name: plugin_name.clone() }
            }
            CommandSource::GlobalUser | CommandSource::AgentsDir => InvocableSource::User,
            CommandSource::ProjectCwd => InvocableSource::Project,
        };

        Self {
            name: command.name.clone(),
            description: command.description.clone(),
            // Commands do not carry a `when_to_use` field in their
            // frontmatter today; leave it as `None`.
            when_to_use: None,
            kind: InvocableKind::Command,
            source,
            // Commands are always model-invocable — the
            // `disable-model-invocation` flag is a skill-only concept.
            disable_model_invocation: false,
            // Commands are user-invocable by definition.
            user_invocable: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{Command, CommandSource, Skill, SkillSource};

    // --- Skill -> InvocableCommand ---------------------------------------

    #[test]
    fn test_from_skill_builtin() {
        let fixture = Skill::new("pdf", "body", "Handle PDF files");

        let actual = InvocableCommand::from(&fixture);

        let expected = InvocableCommand {
            name: "pdf".to_string(),
            description: "Handle PDF files".to_string(),
            when_to_use: None,
            kind: InvocableKind::Skill,
            source: InvocableSource::Builtin,
            disable_model_invocation: false,
            user_invocable: true,
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_from_skill_plugin_preserves_plugin_name() {
        let fixture = Skill::new("demo:pdf", "body", "Handle PDF files")
            .with_source(SkillSource::Plugin { plugin_name: "demo".into() });

        let actual = InvocableCommand::from(&fixture);

        assert_eq!(
            actual.source,
            InvocableSource::Plugin { plugin_name: "demo".into() }
        );
        assert_eq!(actual.kind, InvocableKind::Skill);
    }

    #[test]
    fn test_from_skill_global_user_collapses_to_user() {
        let fixture = Skill::new("s", "b", "d").with_source(SkillSource::GlobalUser);

        let actual = InvocableCommand::from(&fixture);

        assert_eq!(actual.source, InvocableSource::User);
    }

    #[test]
    fn test_from_skill_agents_dir_collapses_to_user() {
        let fixture = Skill::new("s", "b", "d").with_source(SkillSource::AgentsDir);

        let actual = InvocableCommand::from(&fixture);

        assert_eq!(actual.source, InvocableSource::User);
    }

    #[test]
    fn test_from_skill_project_cwd_maps_to_project() {
        let fixture = Skill::new("s", "b", "d").with_source(SkillSource::ProjectCwd);

        let actual = InvocableCommand::from(&fixture);

        assert_eq!(actual.source, InvocableSource::Project);
    }

    #[test]
    fn test_from_skill_preserves_when_to_use() {
        let fixture = Skill::new("s", "b", "d").when_to_use("when the user asks");

        let actual = InvocableCommand::from(&fixture);

        assert_eq!(actual.when_to_use.as_deref(), Some("when the user asks"));
    }

    #[test]
    fn test_from_skill_preserves_invocation_flags() {
        let fixture = Skill {
            name: "s".into(),
            path: None,
            command: "b".into(),
            description: "d".into(),
            resources: vec![],
            source: SkillSource::Builtin,
            when_to_use: None,
            allowed_tools: None,
            disable_model_invocation: true,
            user_invocable: false,
        };

        let actual = InvocableCommand::from(&fixture);

        assert!(actual.disable_model_invocation);
        assert!(!actual.user_invocable);
    }

    // --- Command -> InvocableCommand -------------------------------------

    #[test]
    fn test_from_command_builtin() {
        let fixture = Command::default().name("deploy").description("Ship it");

        let actual = InvocableCommand::from(&fixture);

        let expected = InvocableCommand {
            name: "deploy".to_string(),
            description: "Ship it".to_string(),
            when_to_use: None,
            kind: InvocableKind::Command,
            source: InvocableSource::Builtin,
            disable_model_invocation: false,
            user_invocable: true,
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_from_command_plugin_preserves_plugin_name() {
        let fixture = Command::default()
            .name("demo:deploy")
            .description("Ship it")
            .with_source(CommandSource::Plugin { plugin_name: "demo".into() });

        let actual = InvocableCommand::from(&fixture);

        assert_eq!(
            actual.source,
            InvocableSource::Plugin { plugin_name: "demo".into() }
        );
        assert_eq!(actual.kind, InvocableKind::Command);
    }

    #[test]
    fn test_from_command_global_user_collapses_to_user() {
        let fixture = Command::default()
            .name("x")
            .description("y")
            .with_source(CommandSource::GlobalUser);

        let actual = InvocableCommand::from(&fixture);

        assert_eq!(actual.source, InvocableSource::User);
    }

    #[test]
    fn test_from_command_agents_dir_collapses_to_user() {
        let fixture = Command::default()
            .name("x")
            .description("y")
            .with_source(CommandSource::AgentsDir);

        let actual = InvocableCommand::from(&fixture);

        assert_eq!(actual.source, InvocableSource::User);
    }

    #[test]
    fn test_from_command_project_cwd_maps_to_project() {
        let fixture = Command::default()
            .name("x")
            .description("y")
            .with_source(CommandSource::ProjectCwd);

        let actual = InvocableCommand::from(&fixture);

        assert_eq!(actual.source, InvocableSource::Project);
    }

    #[test]
    fn test_from_command_defaults_invocation_flags() {
        // Commands do not carry disable-model-invocation or user-invocable
        // frontmatter today — they are always invocable by both model and
        // user. Verify the conversion encodes those defaults.
        let fixture = Command::default().name("deploy").description("Ship it");

        let actual = InvocableCommand::from(&fixture);

        assert!(!actual.disable_model_invocation);
        assert!(actual.user_invocable);
    }

    #[test]
    fn test_from_command_when_to_use_is_none() {
        let fixture = Command::default().name("deploy").description("Ship it");

        let actual = InvocableCommand::from(&fixture);

        assert_eq!(actual.when_to_use, None);
    }
}
