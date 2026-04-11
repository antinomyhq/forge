use derive_setters::Setters;
use serde::{Deserialize, Serialize};

/// Where a command was loaded from. Mirrors [`crate::SkillSource`] so that
/// provenance can be attached to every loaded command in the unified
/// listing pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
#[derive(Default)]
pub enum CommandSource {
    /// Compiled into the Forge binary.
    #[default]
    Builtin,
    /// Contributed by an installed plugin.
    Plugin {
        /// Name of the plugin that owns the command.
        plugin_name: String,
    },
    /// User-global command in `~/forge/commands/`.
    GlobalUser,
    /// Command in the shared `~/.agents/commands/` directory.
    AgentsDir,
    /// Project-local command in `./.forge/commands/`.
    ProjectCwd,
}

/// A user-defined command loaded from a Markdown file with YAML frontmatter.
///
/// Commands are discovered from `.md` files in the forge commands directories
/// and made available as slash commands in the UI. The `name` and `description`
/// come from YAML frontmatter; the `prompt` is the Markdown body of the file.
#[derive(Debug, Clone, Default, Deserialize, Setters, PartialEq)]
#[setters(into, strip_option)]
pub struct Command {
    /// The command name used to invoke it (e.g. `github-pr-description`).
    #[serde(default)]
    pub name: String,
    /// Short description shown in the command list.
    #[serde(default)]
    pub description: String,
    /// The prompt template body (Markdown content after the frontmatter).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Origin of the command. Defaults to [`CommandSource::Builtin`] and is
    /// `#[serde(default)]` so frontmatter parsing of legacy `.md` command
    /// files still succeeds without a `source` key.
    #[serde(default)]
    pub source: CommandSource,
}

impl Command {
    /// Builder-style override for [`Command::source`]. Kept separate from
    /// the derived [`Default`] / [`Setters`] surface so that the struct
    /// remains constructible through frontmatter deserialization without a
    /// `source` field.
    pub fn with_source(mut self, source: CommandSource) -> Self {
        self.source = source;
        self
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_command_default_source_is_builtin() {
        let fixture = Command::default();
        assert_eq!(fixture.source, CommandSource::Builtin);
    }

    #[test]
    fn test_command_with_source_plugin() {
        let fixture =
            Command::default().with_source(CommandSource::Plugin { plugin_name: "demo".into() });
        assert_eq!(
            fixture.source,
            CommandSource::Plugin { plugin_name: "demo".into() }
        );
    }

    #[test]
    fn test_command_source_serde_roundtrip() {
        let variants = vec![
            CommandSource::Builtin,
            CommandSource::Plugin { plugin_name: "demo".into() },
            CommandSource::GlobalUser,
            CommandSource::AgentsDir,
            CommandSource::ProjectCwd,
        ];

        for original in variants {
            let json = serde_json::to_string(&original).unwrap();
            let roundtrip: CommandSource = serde_json::from_str(&json).unwrap();
            assert_eq!(roundtrip, original, "roundtrip failed for {json}");
        }
    }

    #[test]
    fn test_command_deserializes_without_source_field() {
        // Frontmatter without a `source` field must still parse cleanly.
        let json = r#"{
            "name": "deploy",
            "description": "Ship it"
        }"#;
        let actual: Command = serde_json::from_str(json).unwrap();
        assert_eq!(actual.name, "deploy");
        assert_eq!(actual.description, "Ship it");
        assert_eq!(actual.source, CommandSource::Builtin);
    }
}
