//! Declarative hook configuration schema (`hooks.json`).
//!
//! These types mirror Claude Code's hook schemas exactly so that a `hooks.json`
//! file authored for Claude Code parses into Forge without modification. Field
//! names, JSON shapes and discriminated-union tags all match the upstream
//! wire format.
//!
//! This module defines only the **data shapes** parsed from `hooks.json`.
//! Execution (shell/http/prompt/agent), matcher evaluation, and hook dispatch
//! live in later phases.
//!
//! References:
//! - Claude Code event enum:
//!   `claude-code/src/entrypoints/sdk/coreSchemas.ts:355-383`
//! - Claude Code hook config: `claude-code/src/schemas/hooks.ts:32-213`

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Top-level `hooks.json` content.
///
/// Maps each lifecycle event to an ordered list of matchers. Each matcher
/// pairs an optional pattern (e.g. `"Bash"` or `"Write|Edit"`) with a list of
/// hook commands to execute when the pattern matches.
///
/// Uses `BTreeMap` for deterministic iteration order, which matters for
/// reproducible hook execution across runs.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(transparent)]
pub struct HooksConfig(pub BTreeMap<HookEventName, Vec<HookMatcher>>);

/// Valid hook event names.
///
/// 27 variants total — matches Claude Code's `HOOK_EVENTS` enum exactly.
/// Several variants (`TeammateIdle`, `TaskCreated`, `TaskCompleted`) are
/// parsed but not currently fired: they are accepted by the parser so that
/// manifests using them don't break.
///
/// Uses Rust's default PascalCase enum serialization, which matches Claude
/// Code's wire format. `Ord` / `PartialOrd` are derived so the enum can be
/// used as a key in the `BTreeMap` inside [`HooksConfig`].
#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum HookEventName {
    PreToolUse,
    PostToolUse,
    PostToolUseFailure,
    Notification,
    UserPromptSubmit,
    SessionStart,
    SessionEnd,
    Stop,
    StopFailure,
    SubagentStart,
    SubagentStop,
    PreCompact,
    PostCompact,
    PermissionRequest,
    PermissionDenied,
    Setup,
    /// Parsed but not currently fired.
    TeammateIdle,
    /// Parsed but not currently fired.
    TaskCreated,
    /// Parsed but not currently fired.
    TaskCompleted,
    Elicitation,
    ElicitationResult,
    ConfigChange,
    WorktreeCreate,
    WorktreeRemove,
    InstructionsLoaded,
    CwdChanged,
    FileChanged,
}

impl HookEventName {
    /// Returns the PascalCase wire name matching Claude Code's event format.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PreToolUse => "PreToolUse",
            Self::PostToolUse => "PostToolUse",
            Self::PostToolUseFailure => "PostToolUseFailure",
            Self::Notification => "Notification",
            Self::UserPromptSubmit => "UserPromptSubmit",
            Self::SessionStart => "SessionStart",
            Self::SessionEnd => "SessionEnd",
            Self::Stop => "Stop",
            Self::StopFailure => "StopFailure",
            Self::SubagentStart => "SubagentStart",
            Self::SubagentStop => "SubagentStop",
            Self::PreCompact => "PreCompact",
            Self::PostCompact => "PostCompact",
            Self::PermissionRequest => "PermissionRequest",
            Self::PermissionDenied => "PermissionDenied",
            Self::Setup => "Setup",
            Self::TeammateIdle => "TeammateIdle",
            Self::TaskCreated => "TaskCreated",
            Self::TaskCompleted => "TaskCompleted",
            Self::Elicitation => "Elicitation",
            Self::ElicitationResult => "ElicitationResult",
            Self::ConfigChange => "ConfigChange",
            Self::WorktreeCreate => "WorktreeCreate",
            Self::WorktreeRemove => "WorktreeRemove",
            Self::InstructionsLoaded => "InstructionsLoaded",
            Self::CwdChanged => "CwdChanged",
            Self::FileChanged => "FileChanged",
        }
    }

    /// Returns `true` for events that support `FORGE_ENV_FILE` write-back.
    ///
    /// Hooks for these events can write `KEY=VALUE` pairs to the file
    /// specified in `FORGE_ENV_FILE`; the runtime reads them back and
    /// merges them into the session environment cache.
    pub fn supports_env_file(&self) -> bool {
        matches!(
            self,
            Self::SessionStart | Self::Setup | Self::CwdChanged | Self::FileChanged
        )
    }
}

/// A single entry inside a `hooks.json` event list.
///
/// The optional `matcher` field filters which tool calls (or other event
/// payloads) trigger the contained `hooks`. An omitted or empty matcher
/// matches everything.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HookMatcher {
    /// Pattern matched against tool name (or other event-specific key).
    /// Supports exact strings, pipe-separated alternatives (`"Write|Edit"`),
    /// glob-like wildcards (`"*"`), and JavaScript-style regex literals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matcher: Option<String>,
    /// Commands to execute when `matcher` matches.
    pub hooks: Vec<HookCommand>,
}

/// A single hook command. The `type` tag discriminates between the four
/// executor kinds: shell command, LLM prompt, HTTP webhook, or sub-agent.
///
/// Claude Code uses lowercase tag values (`"command"`, `"prompt"`, `"http"`,
/// `"agent"`), so we mirror that with `rename_all = "lowercase"`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum HookCommand {
    /// Shell subprocess hook.
    Command(ShellHookCommand),
    /// Single LLM prompt hook (runs a small model call).
    Prompt(PromptHookCommand),
    /// HTTP webhook hook.
    Http(HttpHookCommand),
    /// Full sub-agent hook (spawns an agent loop).
    Agent(AgentHookCommand),
}

/// Shell subprocess hook — the most common kind.
///
/// Claude Code stores this as camelCase in `hooks.json`, so we apply
/// `rename_all = "camelCase"` to the struct. A few fields have bespoke
/// renames to match the exact wire names:
/// - `condition` is wire-named `if` (a Rust keyword)
/// - `async_mode` is wire-named `async` (also a keyword)
/// - `async_rewake` is wire-named `asyncRewake` to preserve camelCase when
///   mixed with the `async` rename.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShellHookCommand {
    /// The shell command to run.
    pub command: String,
    /// Optional Claude-Code-style condition string (e.g. `"Bash(git *)"`).
    /// Evaluated before spawning; if it doesn't match, the hook is skipped.
    #[serde(default, rename = "if", skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// Shell to use. Defaults to `bash` on Unix, `powershell` on Windows
    /// when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<ShellType>,
    /// Timeout in seconds. Defaults to 30 seconds when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    /// Optional status-line message shown while the hook runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
    /// If `true`, this hook fires at most once per session.
    #[serde(default)]
    pub once: bool,
    /// If `true`, run the hook in the background and return immediately.
    /// Wire field is `async` (a Rust keyword), hence the rename.
    #[serde(default, rename = "async")]
    pub async_mode: bool,
    /// If `true`, an async hook that later exits with code 2 wakes the
    /// agent loop. Requires `async_mode: true`.
    #[serde(default, rename = "asyncRewake")]
    pub async_rewake: bool,
}

/// Which shell to use for a [`ShellHookCommand`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShellType {
    Bash,
    Powershell,
}

/// LLM prompt hook — invokes a single chat completion with the given prompt.
///
/// The subprocess model is bypassed; instead Forge runs a small-fast model
/// (e.g. Haiku-tier) and parses its JSON response as [`crate::HookOutput`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PromptHookCommand {
    /// The prompt sent to the model. May contain `$ARGUMENTS` which is
    /// substituted with the serialized `HookInput` before dispatch.
    pub prompt: String,
    /// Optional Claude-Code-style condition string.
    #[serde(default, rename = "if", skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// Timeout in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    /// Optional model override (e.g. `"claude-3-haiku-20240307"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Optional status-line message shown while the hook runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
    /// If `true`, this hook fires at most once per session.
    #[serde(default)]
    pub once: bool,
}

/// HTTP webhook hook — POSTs the `HookInput` JSON to a URL and parses the
/// response body as [`crate::HookOutput`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HttpHookCommand {
    /// Target URL.
    pub url: String,
    /// Optional Claude-Code-style condition string.
    #[serde(default, rename = "if", skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// Timeout in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    /// Extra HTTP headers. Values may reference environment variables via
    /// `$VAR` / `${VAR}` — only names in `allowed_env_vars` are substituted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<BTreeMap<String, String>>,
    /// Whitelist of environment variable names that may be substituted into
    /// header values. Defends against accidentally leaking secrets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_env_vars: Option<Vec<String>>,
    /// Optional status-line message shown while the hook runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
    /// If `true`, this hook fires at most once per session.
    #[serde(default)]
    pub once: bool,
}

/// Sub-agent hook — spawns a full agent loop (not a single model call).
///
/// Functionally similar to [`PromptHookCommand`] but uses the agent executor,
/// so the hook can take multiple turns and invoke tools. Used for agentic
/// verification scenarios like "Verify tests pass before continuing".
///
/// Full execution is not yet implemented; the type exists so manifests
/// parse correctly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentHookCommand {
    /// The prompt sent to the sub-agent.
    pub prompt: String,
    /// Optional Claude-Code-style condition string.
    #[serde(default, rename = "if", skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// Timeout in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    /// Optional model override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Optional status-line message shown while the hook runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
    /// If `true`, this hook fires at most once per session.
    #[serde(default)]
    pub once: bool,
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_hooks_config_parses_empty_object() {
        let fixture = "{}";
        let actual: HooksConfig = serde_json::from_str(fixture).unwrap();
        let expected = HooksConfig(BTreeMap::new());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_hooks_config_parses_pre_tool_use_shell_hook() {
        let fixture = r#"{
            "PreToolUse": [{
                "matcher": "Bash",
                "hooks": [{"type": "command", "command": "echo hi"}]
            }]
        }"#;
        let actual: HooksConfig = serde_json::from_str(fixture).unwrap();

        let matchers = actual.0.get(&HookEventName::PreToolUse).unwrap();
        assert_eq!(matchers.len(), 1);
        assert_eq!(matchers[0].matcher.as_deref(), Some("Bash"));
        assert_eq!(matchers[0].hooks.len(), 1);
        match &matchers[0].hooks[0] {
            HookCommand::Command(shell) => {
                assert_eq!(shell.command, "echo hi");
                assert_eq!(shell.condition, None);
                assert_eq!(shell.shell, None);
                assert!(!shell.once);
                assert!(!shell.async_mode);
                assert!(!shell.async_rewake);
            }
            other => panic!("expected Command variant, got {other:?}"),
        }
    }

    #[test]
    fn test_hook_matcher_without_matcher_field_defaults_to_none() {
        let fixture = r#"{
            "hooks": [{"type": "command", "command": "true"}]
        }"#;
        let actual: HookMatcher = serde_json::from_str(fixture).unwrap();
        assert_eq!(actual.matcher, None);
        assert_eq!(actual.hooks.len(), 1);
    }

    #[test]
    fn test_hook_command_discriminated_union_parses_all_four_kinds() {
        // command
        let cmd: HookCommand =
            serde_json::from_str(r#"{"type":"command","command":"ls"}"#).unwrap();
        assert!(matches!(cmd, HookCommand::Command(_)));

        // prompt
        let prompt: HookCommand =
            serde_json::from_str(r#"{"type":"prompt","prompt":"Summarize the diff"}"#).unwrap();
        assert!(matches!(prompt, HookCommand::Prompt(_)));

        // http
        let http: HookCommand =
            serde_json::from_str(r#"{"type":"http","url":"https://example.com/webhook"}"#).unwrap();
        assert!(matches!(http, HookCommand::Http(_)));

        // agent
        let agent: HookCommand =
            serde_json::from_str(r#"{"type":"agent","prompt":"Verify tests"}"#).unwrap();
        assert!(matches!(agent, HookCommand::Agent(_)));
    }

    #[test]
    fn test_shell_hook_command_parses_async_and_async_rewake() {
        let fixture = r#"{
            "type": "command",
            "command": "long-running.sh",
            "async": true,
            "asyncRewake": true
        }"#;
        let actual: HookCommand = serde_json::from_str(fixture).unwrap();
        match actual {
            HookCommand::Command(shell) => {
                assert!(shell.async_mode);
                assert!(shell.async_rewake);
            }
            other => panic!("expected Command variant, got {other:?}"),
        }
    }

    #[test]
    fn test_shell_hook_command_if_field_aliases_to_condition() {
        let fixture = r#"{
            "type": "command",
            "command": "check.sh",
            "if": "Bash(git *)"
        }"#;
        let actual: HookCommand = serde_json::from_str(fixture).unwrap();
        match actual {
            HookCommand::Command(shell) => {
                assert_eq!(shell.condition.as_deref(), Some("Bash(git *)"));
            }
            other => panic!("expected Command variant, got {other:?}"),
        }
    }

    #[test]
    fn test_shell_hook_command_roundtrips_if_back_to_wire_name() {
        let shell = ShellHookCommand {
            command: "x".to_string(),
            condition: Some("Bash(git *)".to_string()),
            shell: Some(ShellType::Bash),
            timeout: Some(10),
            status_message: None,
            once: false,
            async_mode: false,
            async_rewake: false,
        };
        let json = serde_json::to_value(&shell).unwrap();
        // Wire field must be "if", not "condition".
        assert_eq!(json.get("if").and_then(|v| v.as_str()), Some("Bash(git *)"));
        assert_eq!(json.get("condition"), None);
        assert_eq!(json.get("shell").and_then(|v| v.as_str()), Some("bash"));
    }

    #[test]
    fn test_hook_event_name_serializes_as_pascal_case() {
        let name = HookEventName::PreToolUse;
        let json = serde_json::to_string(&name).unwrap();
        assert_eq!(json, "\"PreToolUse\"");

        let parsed: HookEventName = serde_json::from_str("\"PostToolUseFailure\"").unwrap();
        assert_eq!(parsed, HookEventName::PostToolUseFailure);
    }

    #[test]
    fn test_unfired_event_variants_parse_successfully() {
        // These three events are not currently fired but the parser must still
        // accept them so Claude-Code-authored manifests load without error.
        let fixture = r#"{
            "TeammateIdle": [],
            "TaskCreated": [],
            "TaskCompleted": []
        }"#;
        let actual: HooksConfig = serde_json::from_str(fixture).unwrap();
        assert!(actual.0.contains_key(&HookEventName::TeammateIdle));
        assert!(actual.0.contains_key(&HookEventName::TaskCreated));
        assert!(actual.0.contains_key(&HookEventName::TaskCompleted));
    }
}
