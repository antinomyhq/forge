//! Memory/instructions domain types for the plugin hook system.
//!
//! These types carry metadata about files loaded into the agent's
//! context via the `InstructionsLoaded` lifecycle hook. Plugins
//! filter on `load_reason` to react to specific load triggers.
//!
//! # Ownership note
//!
//! [`MemoryType`] and [`InstructionsLoadReason`] were originally defined
//! inline next to [`crate::InstructionsLoadedPayload`] in `hook_payloads.rs`
//! They live here so that the in-process [`LoadedInstructions`] struct
//! (also defined in this module) can reuse the classification enums
//! without creating a circular dependency back into `hook_payloads`.
//! The payload struct itself continues to live in `hook_payloads.rs`
//! unchanged and imports these enums via the crate root re-export.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Source category of an instructions file. Matches Claude Code's
/// `CLAUDE_MD_MEMORY_TYPES` vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    /// `~/forge/AGENTS.md` + `~/forge/rules/*.md`. Per-user
    /// customisation applied across all projects.
    User,
    /// `<repo>/AGENTS.md` + nested ancestor `AGENTS.md` files.
    /// Shared per-project rules committed to the repo.
    Project,
    /// `<repo>/AGENTS.local.md`. Gitignored per-checkout rules.
    Local,
    /// `/etc/forge/AGENTS.md`. Admin-managed policy instructions.
    Managed,
}

impl MemoryType {
    /// The wire-format string used when serialising this memory
    /// type into a plugin hook payload. Matches Claude Code exactly
    /// so plugins that filter on memory_type work unchanged.
    pub fn as_wire_str(&self) -> &'static str {
        match self {
            MemoryType::User => "user",
            MemoryType::Project => "project",
            MemoryType::Local => "local",
            MemoryType::Managed => "managed",
        }
    }
}

/// Why a given instructions file was loaded. Plugins can install
/// hook matchers that fire only for specific reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstructionsLoadReason {
    /// File was loaded at session start as part of the static memory
    /// layer. This is the only currently used reason.
    SessionStart,
    /// File was loaded because the agent touched a file in a directory
    /// that contained a nested `AGENTS.md`.
    NestedTraversal,
    /// Conditional rule with a `paths:` glob matched a file the agent
    /// touched.
    PathGlobMatch,
    /// File was pulled in via an `@include path/to/other.md` directive
    /// in another instructions file.
    Include,
    /// File was reloaded after a compaction discarded the prior
    /// context.
    Compact,
}

impl InstructionsLoadReason {
    /// Wire-format string matching Claude Code's load reason enum.
    pub fn as_wire_str(&self) -> &'static str {
        match self {
            InstructionsLoadReason::SessionStart => "session_start",
            InstructionsLoadReason::NestedTraversal => "nested_traversal",
            InstructionsLoadReason::PathGlobMatch => "path_glob_match",
            InstructionsLoadReason::Include => "include",
            InstructionsLoadReason::Compact => "compact",
        }
    }
}

/// Optional YAML frontmatter on an instructions file. Parsed so
/// round-tripping via serde survives. The `paths` and `include`
/// fields are not yet acted on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct InstructionsFrontmatter {
    /// Glob patterns that activate this rule when the agent touches
    /// a matching file. `None` means unconditional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paths: Option<Vec<String>>,
    /// `@include` target paths to recursively load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<String>>,
}

/// A single instructions file that was loaded into the agent's
/// context, enriched with classification metadata so the hook fire
/// site can populate an `InstructionsLoadedPayload` without
/// re-reading the filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedInstructions {
    /// Absolute path to the source file.
    pub file_path: PathBuf,
    /// Source category — user / project / local / managed.
    pub memory_type: MemoryType,
    /// Trigger for this load. Currently only `SessionStart` is emitted.
    pub load_reason: InstructionsLoadReason,
    /// File contents after frontmatter has been stripped. This is
    /// the text the system prompt injects.
    pub content: String,
    /// Parsed frontmatter (if any). `None` when the file had no
    /// YAML frontmatter block. Parsed but not yet acted on.
    pub frontmatter: Option<InstructionsFrontmatter>,
    /// Path glob patterns copied out of the frontmatter for
    /// convenience on the hook payload. `None` when the frontmatter
    /// had no `paths:` field.
    pub globs: Option<Vec<String>>,
    /// Absolute path of the file whose access triggered loading this
    /// instructions file. `None` for `SessionStart` loads.
    pub trigger_file_path: Option<PathBuf>,
    /// Absolute path of the parent instructions file when this one
    /// was pulled in via `@include`. `None` for top-level loads.
    pub parent_file_path: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_memory_type_as_wire_str_all_variants() {
        // Fixture — all four memory-type variants with their expected
        // Claude-Code-compatible wire strings.
        let fixture = [
            (MemoryType::User, "user"),
            (MemoryType::Project, "project"),
            (MemoryType::Local, "local"),
            (MemoryType::Managed, "managed"),
        ];

        // Act / Assert — each variant must round-trip to its wire
        // string and serde must agree on the same form.
        for (variant, expected) in fixture {
            let actual = variant.as_wire_str();
            assert_eq!(actual, expected);

            let json = serde_json::to_string(&variant).unwrap();
            let expected_json = format!("\"{expected}\"");
            assert_eq!(json, expected_json);

            let roundtrip: MemoryType = serde_json::from_str(&json).unwrap();
            assert_eq!(roundtrip, variant);
        }
    }

    #[test]
    fn test_instructions_load_reason_as_wire_str_all_variants() {
        // Fixture — every load-reason variant paired with its wire
        // string. `SessionStart` is the only currently used reason;
        // the rest must still round-trip.
        let fixture = [
            (InstructionsLoadReason::SessionStart, "session_start"),
            (InstructionsLoadReason::NestedTraversal, "nested_traversal"),
            (InstructionsLoadReason::PathGlobMatch, "path_glob_match"),
            (InstructionsLoadReason::Include, "include"),
            (InstructionsLoadReason::Compact, "compact"),
        ];

        // Act / Assert — verify `as_wire_str` matches the documented
        // string and that serde serialises/deserialises to the same
        // wire form.
        for (variant, expected) in fixture {
            let actual = variant.as_wire_str();
            assert_eq!(actual, expected);

            let json = serde_json::to_string(&variant).unwrap();
            let expected_json = format!("\"{expected}\"");
            assert_eq!(json, expected_json);

            let roundtrip: InstructionsLoadReason = serde_json::from_str(&json).unwrap();
            assert_eq!(roundtrip, variant);
        }
    }
}
