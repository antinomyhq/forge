//! Hook subprocess I/O types — the JSON payloads sent to hook executables
//! on stdin and received back on stdout.
//!
//! Field names mirror Claude Code's wire format exactly so that a hook
//! binary written for Claude Code keeps working in Forge. This means the
//! input side is snake_case (`session_id`, `tool_name`, ...) while the
//! output side is camelCase (`hookSpecificOutput`, `permissionDecision`,
//! ...). Both sides also use literal JSON keys that collide with Rust
//! keywords (`async`, `continue`, `if`), which we handle with `#[serde(rename =
//! ...)]`.
//!
//! The types in this module define only the **shapes**. Actual subprocess
//! execution, streaming, and timeout enforcement live in later phases.
//!
//! References:
//! - Claude Code event schemas (input):
//!   `claude-code/src/entrypoints/sdk/coreSchemas.ts:387-796`
//! - Claude Code output schemas:
//!   `claude-code/src/entrypoints/sdk/coreSchemas.ts:799-974`

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ---------- HookInput (stdin) ----------

/// Fields inherited by every hook input event.
///
/// These are flattened into [`HookInput`] alongside an event-specific
/// [`HookInputPayload`] so the serialized JSON contains all base and
/// payload fields at the top level (matching Claude Code's flat layout).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct HookInputBase {
    /// Current session ID.
    pub session_id: String,
    /// Absolute path to the transcript file for this session.
    pub transcript_path: PathBuf,
    /// Current working directory.
    pub cwd: PathBuf,
    /// Optional permission mode (`"default"`, `"acceptEdits"`, ...).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    /// Optional agent ID when the event originated from a sub-agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Optional agent type (e.g. `"forge"`, `"code-reviewer"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    /// Literal name of the event, e.g. `"PreToolUse"`. Duplicated here for
    /// wire compatibility — Claude Code emits this field as a sibling of
    /// the payload fields.
    pub hook_event_name: String,
}

/// Full hook input payload written to a hook subprocess's stdin.
///
/// Combines [`HookInputBase`] (common fields) with an event-specific
/// [`HookInputPayload`] via `#[serde(flatten)]`. The resulting JSON is flat,
/// with base and payload fields interleaved at the top level.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HookInput {
    #[serde(flatten)]
    pub base: HookInputBase,
    #[serde(flatten)]
    pub payload: HookInputPayload,
}

/// Event-specific hook input payload.
///
/// The `#[serde(untagged)]` attribute means serde picks the variant based on
/// the presence of the fields — there's no explicit discriminator tag on
/// the wire, because the parent [`HookInputBase::hook_event_name`] plays
/// that role.
///
/// The final `Generic(serde_json::Value)` variant catches any event shape
/// we haven't modeled yet (including the `Teammates`/`Tasks` events that
/// are not currently fired). This keeps the parser forward-compatible.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(untagged, rename_all = "snake_case")]
pub enum HookInputPayload {
    PreToolUse {
        tool_name: String,
        tool_input: serde_json::Value,
        tool_use_id: String,
    },
    PostToolUse {
        tool_name: String,
        tool_input: serde_json::Value,
        tool_response: serde_json::Value,
        tool_use_id: String,
    },
    PostToolUseFailure {
        tool_name: String,
        tool_input: serde_json::Value,
        tool_use_id: String,
        error: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_interrupt: Option<bool>,
    },
    UserPromptSubmit {
        prompt: String,
    },
    SessionStart {
        source: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    SessionEnd {
        reason: String,
    },
    Stop {
        stop_hook_active: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        last_assistant_message: Option<String>,
    },
    StopFailure {
        error: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        error_details: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        last_assistant_message: Option<String>,
    },
    PreCompact {
        trigger: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        custom_instructions: Option<String>,
    },
    PostCompact {
        trigger: String,
        compact_summary: String,
    },
    Notification {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        notification_type: String,
    },
    Setup {
        trigger: String,
    },
    ConfigChange {
        source: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_path: Option<std::path::PathBuf>,
    },
    SubagentStart {
        agent_id: String,
        agent_type: String,
    },
    SubagentStop {
        agent_id: String,
        agent_type: String,
        agent_transcript_path: PathBuf,
        stop_hook_active: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        last_assistant_message: Option<String>,
    },
    PermissionRequest {
        tool_name: String,
        tool_input: serde_json::Value,
        permission_suggestions: Vec<crate::PermissionUpdate>,
    },
    PermissionDenied {
        tool_name: String,
        tool_input: serde_json::Value,
        tool_use_id: String,
        reason: String,
    },
    CwdChanged {
        old_cwd: PathBuf,
        new_cwd: PathBuf,
    },
    FileChanged {
        file_path: PathBuf,
        event: String,
    },
    WorktreeCreate {
        name: String,
    },
    WorktreeRemove {
        worktree_path: PathBuf,
    },
    InstructionsLoaded {
        file_path: PathBuf,
        memory_type: String,
        load_reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        globs: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        trigger_file_path: Option<PathBuf>,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_file_path: Option<PathBuf>,
    },
    Elicitation {
        #[serde(rename = "mcp_server_name")]
        server_name: String,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        requested_schema: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        elicitation_id: Option<String>,
    },
    ElicitationResult {
        #[serde(rename = "mcp_server_name")]
        server_name: String,
        action: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        elicitation_id: Option<String>,
    },
    /// Fallback for event payload shapes we haven't modeled yet — including
    /// unrecognized v4 events like `TeammateIdle`. The raw JSON is preserved.
    Generic(serde_json::Value),
}

// ---------- HookOutput (stdout) ----------

/// Hook output as read from a subprocess's stdout.
///
/// A hook may respond in one of two shapes:
/// - [`AsyncHookOutput`] — short ack indicating the hook will complete in the
///   background.
/// - [`SyncHookOutput`] — the full response with decision, continuation, and
///   event-specific augmentations.
///
/// `#[serde(untagged)]` picks the variant by structural matching. The
/// `Async` variant is listed first so a payload containing `"async": true`
/// matches it before the broader sync shape.
///
/// Output is `Deserialize`-only: Forge never writes these JSON values,
/// it only parses them from hook subprocess stdout.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum HookOutput {
    Async(AsyncHookOutput),
    Sync(SyncHookOutput),
}

/// Short ack returned by a hook that is running asynchronously.
///
/// The `is_async` field must be `true` on the wire. The optional
/// `async_timeout` lets the hook cap how long Forge waits before assuming
/// the async job died.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AsyncHookOutput {
    #[serde(rename = "async")]
    pub is_async: bool,
    #[serde(default)]
    pub async_timeout: Option<u64>,
}

/// Full synchronous hook response.
///
/// All fields are optional so hooks can opt in to just the pieces they
/// need. The `continue` / `decision` fields use explicit renames because
/// `continue` is a Rust keyword.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncHookOutput {
    /// Whether the orchestrator should continue the loop after this hook.
    /// `Some(false)` halts processing (e.g. stop the agent turn).
    #[serde(default, rename = "continue")]
    pub should_continue: Option<bool>,
    /// If `Some(true)`, the hook's stdout is suppressed from the user log.
    #[serde(default)]
    pub suppress_output: Option<bool>,
    /// Optional message explaining why the agent turn was stopped.
    #[serde(default)]
    pub stop_reason: Option<String>,
    /// Global approve/block decision (used for PreToolUse gating).
    #[serde(default)]
    pub decision: Option<HookDecision>,
    /// System message to inject into the conversation.
    #[serde(default)]
    pub system_message: Option<String>,
    /// Free-form reason string shown to the user.
    #[serde(default)]
    pub reason: Option<String>,
    /// Event-specific augmentation — populated for PreToolUse permission
    /// decisions, PostToolUse overrides, UserPromptSubmit context, etc.
    #[serde(default)]
    pub hook_specific_output: Option<HookSpecificOutput>,
}

/// Global hook decision used by [`SyncHookOutput::decision`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HookDecision {
    Approve,
    Block,
}

/// Event-specific hook output augmentations.
///
/// Discriminated by the `hookEventName` JSON key (note: this one is
/// camelCase even though the input side uses snake_case — that's the
/// asymmetry Claude Code ships with). Currently models the most common
/// variants; the enum is extended as more events are wired up.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "hookEventName")]
pub enum HookSpecificOutput {
    PreToolUse {
        #[serde(default, rename = "permissionDecision")]
        permission_decision: Option<PermissionDecision>,
        #[serde(default, rename = "permissionDecisionReason")]
        permission_decision_reason: Option<String>,
        #[serde(default, rename = "updatedInput")]
        updated_input: Option<serde_json::Value>,
        #[serde(default, rename = "additionalContext")]
        additional_context: Option<String>,
    },
    PostToolUse {
        #[serde(default, rename = "additionalContext")]
        additional_context: Option<String>,
        #[serde(default, rename = "updatedMCPToolOutput")]
        updated_mcp_tool_output: Option<serde_json::Value>,
    },
    UserPromptSubmit {
        #[serde(default, rename = "additionalContext")]
        additional_context: Option<String>,
    },
    SessionStart {
        #[serde(default, rename = "additionalContext")]
        additional_context: Option<String>,
        #[serde(default, rename = "initialUserMessage")]
        initial_user_message: Option<String>,
        #[serde(default, rename = "watchPaths")]
        watch_paths: Option<Vec<PathBuf>>,
    },
    /// Plugin-driven output for a `PermissionRequest` event. Mirrors
    /// Claude Code's wire shape (`claude-code/src/utils/hooks.ts:3480-3560`)
    /// and is consumed by [`crate::AggregatedHookResult::merge`] inside
    /// the permission fire site.
    PermissionRequest {
        #[serde(default, rename = "permissionDecision")]
        permission_decision: Option<PermissionDecision>,
        #[serde(default, rename = "permissionDecisionReason")]
        permission_decision_reason: Option<String>,
        #[serde(default, rename = "updatedInput")]
        updated_input: Option<serde_json::Value>,
        /// Updated permission scopes for tool/path — merged into
        /// `AggregatedHookResult.updated_permissions` last-write-wins.
        #[serde(default, rename = "updatedPermissions")]
        updated_permissions: Option<serde_json::Value>,
        /// If `true`, plugin requests an interactive session interrupt.
        #[serde(default)]
        interrupt: Option<bool>,
        /// If `true`, plugin requests the caller to re-issue the
        /// permission prompt (e.g. after refreshing credentials).
        #[serde(default)]
        retry: Option<bool>,
        /// Claude Code nested decision object. When present, fields are
        /// extracted from within the decision variant during merge.
        #[serde(default)]
        decision: Option<PermissionRequestDecision>,
    },
    /// Plugin-driven output for a `WorktreeCreate` event. Mirrors
    /// Claude Code's wire shape (`claude-code/src/utils/hooks.ts:4956`)
    /// where a plugin can hand back a custom worktree path that the
    /// `--worktree` CLI flag uses instead of falling back to the
    /// built-in `git worktree add` path. Consumed by
    /// [`crate::AggregatedHookResult::merge`] last-write-wins.
    WorktreeCreate {
        #[serde(default, rename = "worktreePath")]
        worktree_path: Option<PathBuf>,
    },
    Setup {
        #[serde(default, rename = "additionalContext")]
        additional_context: Option<String>,
    },
    SubagentStart {
        #[serde(default, rename = "additionalContext")]
        additional_context: Option<String>,
    },
    PostToolUseFailure {
        #[serde(default, rename = "additionalContext")]
        additional_context: Option<String>,
    },
    PermissionDenied {
        #[serde(default)]
        retry: Option<bool>,
    },
    Notification {
        #[serde(default, rename = "additionalContext")]
        additional_context: Option<String>,
    },
    Elicitation {
        #[serde(default)]
        action: Option<String>,
        #[serde(default)]
        content: Option<serde_json::Value>,
    },
    ElicitationResult {
        #[serde(default)]
        action: Option<String>,
        #[serde(default)]
        content: Option<serde_json::Value>,
    },
    CwdChanged {
        #[serde(default, rename = "watchPaths")]
        watch_paths: Option<Vec<PathBuf>>,
    },
    FileChanged {
        #[serde(default, rename = "watchPaths")]
        watch_paths: Option<Vec<PathBuf>>,
    },
}

/// Nested permission decision object matching Claude Code's
/// `PermissionRequestHookSpecificOutputSchema`. Tagged on `"behavior"`.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "behavior", rename_all = "lowercase")]
pub enum PermissionRequestDecision {
    Allow {
        #[serde(default, rename = "updatedInput")]
        updated_input: Option<serde_json::Value>,
        #[serde(default, rename = "updatedPermissions")]
        updated_permissions: Option<serde_json::Value>,
    },
    Deny {
        #[serde(default)]
        message: Option<String>,
        #[serde(default)]
        interrupt: Option<bool>,
    },
}

/// Permission decision returned by PreToolUse hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionDecision {
    Allow,
    Deny,
    Ask,
}

// ---------- Prompt Request Protocol (bidirectional stdin) ----------

/// A prompt request emitted by a hook process via stdout.
///
/// The Claude Code hook protocol allows hooks to request interactive prompts
/// during execution. The runtime parses these from stdout line-by-line, shows
/// the prompt to the user, and writes the response back to the hook's stdin.
///
/// Reference: `claude-code/src/utils/hooks.ts:1068-1109`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookPromptRequest {
    pub prompt: HookPromptPayload,
}

/// Payload inside a [`HookPromptRequest`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookPromptPayload {
    /// The type of prompt: `"confirm"`, `"input"`, or `"select"`.
    #[serde(rename = "type")]
    pub prompt_type: String,
    /// The message to display to the user.
    pub message: String,
    /// Default value (optional).
    #[serde(default)]
    pub default: Option<String>,
    /// Options for `select`-type prompts.
    #[serde(default)]
    pub options: Option<Vec<String>>,
}

/// Response sent back to the hook process via stdin after the user answers a
/// prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookPromptResponse {
    pub response: String,
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    fn sample_base(event: &str) -> HookInputBase {
        HookInputBase {
            session_id: "sess-123".to_string(),
            transcript_path: PathBuf::from("/tmp/transcript.json"),
            cwd: PathBuf::from("/home/user/project"),
            permission_mode: None,
            agent_id: None,
            agent_type: None,
            hook_event_name: event.to_string(),
        }
    }

    #[test]
    fn test_hook_input_serializes_pre_tool_use_with_snake_case_fields() {
        let input = HookInput {
            base: sample_base("PreToolUse"),
            payload: HookInputPayload::PreToolUse {
                tool_name: "Bash".to_string(),
                tool_input: json!({"command": "ls -la"}),
                tool_use_id: "toolu_01".to_string(),
            },
        };
        let json = serde_json::to_value(&input).unwrap();

        // Base fields (snake_case)
        assert_eq!(json["session_id"], "sess-123");
        assert_eq!(json["transcript_path"], "/tmp/transcript.json");
        assert_eq!(json["cwd"], "/home/user/project");
        assert_eq!(json["hook_event_name"], "PreToolUse");

        // Payload fields (also snake_case, flattened)
        assert_eq!(json["tool_name"], "Bash");
        assert_eq!(json["tool_input"]["command"], "ls -la");
        assert_eq!(json["tool_use_id"], "toolu_01");

        // Optional fields that are `None` must be absent
        assert!(json.get("permission_mode").is_none());
        assert!(json.get("agent_id").is_none());
        assert!(json.get("agent_type").is_none());
    }

    #[test]
    fn test_hook_input_serializes_post_tool_use_with_tool_response() {
        let input = HookInput {
            base: sample_base("PostToolUse"),
            payload: HookInputPayload::PostToolUse {
                tool_name: "Write".to_string(),
                tool_input: json!({"path": "/x.txt"}),
                tool_response: json!({"ok": true}),
                tool_use_id: "toolu_02".to_string(),
            },
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["tool_name"], "Write");
        assert_eq!(json["tool_response"]["ok"], true);
    }

    #[test]
    fn test_hook_input_serializes_user_prompt_submit() {
        let input = HookInput {
            base: sample_base("UserPromptSubmit"),
            payload: HookInputPayload::UserPromptSubmit { prompt: "Hello forge".to_string() },
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["hook_event_name"], "UserPromptSubmit");
        assert_eq!(json["prompt"], "Hello forge");
    }

    #[test]
    fn test_hook_input_generic_payload_falls_through() {
        let input = HookInput {
            base: sample_base("TeammateIdle"),
            payload: HookInputPayload::Generic(json!({"idle_for": 42})),
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["hook_event_name"], "TeammateIdle");
        assert_eq!(json["idle_for"], 42);
    }

    #[test]
    fn test_hook_output_parses_async_shape() {
        let fixture = r#"{"async": true, "asyncTimeout": 60}"#;
        let actual: HookOutput = serde_json::from_str(fixture).unwrap();
        match actual {
            HookOutput::Async(async_out) => {
                assert!(async_out.is_async);
                assert_eq!(async_out.async_timeout, Some(60));
            }
            other => panic!("expected Async variant, got {other:?}"),
        }
    }

    #[test]
    fn test_hook_output_parses_sync_shape_with_continue_and_pre_tool_use() {
        let fixture = r#"{
            "continue": true,
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow"
            }
        }"#;
        let actual: HookOutput = serde_json::from_str(fixture).unwrap();
        match actual {
            HookOutput::Sync(sync) => {
                assert_eq!(sync.should_continue, Some(true));
                match sync.hook_specific_output {
                    Some(HookSpecificOutput::PreToolUse { permission_decision, .. }) => {
                        assert_eq!(permission_decision, Some(PermissionDecision::Allow));
                    }
                    other => panic!("expected PreToolUse specific output, got {other:?}"),
                }
            }
            other => panic!("expected Sync variant, got {other:?}"),
        }
    }

    #[test]
    fn test_hook_output_sync_parses_decision_block() {
        let fixture = r#"{"decision": "block", "reason": "policy violation"}"#;
        let actual: HookOutput = serde_json::from_str(fixture).unwrap();
        match actual {
            HookOutput::Sync(sync) => {
                assert_eq!(sync.decision, Some(HookDecision::Block));
                assert_eq!(sync.reason.as_deref(), Some("policy violation"));
            }
            other => panic!("expected Sync variant, got {other:?}"),
        }
    }

    #[test]
    fn test_hook_output_sync_parses_post_tool_use_specific_output() {
        let fixture = r#"{
            "hookSpecificOutput": {
                "hookEventName": "PostToolUse",
                "additionalContext": "cached result",
                "updatedMCPToolOutput": {"content": "override"}
            }
        }"#;
        let actual: HookOutput = serde_json::from_str(fixture).unwrap();
        match actual {
            HookOutput::Sync(sync) => match sync.hook_specific_output {
                Some(HookSpecificOutput::PostToolUse {
                    additional_context,
                    updated_mcp_tool_output,
                }) => {
                    assert_eq!(additional_context.as_deref(), Some("cached result"));
                    assert_eq!(updated_mcp_tool_output.unwrap()["content"], "override");
                }
                other => panic!("expected PostToolUse specific output, got {other:?}"),
            },
            other => panic!("expected Sync variant, got {other:?}"),
        }
    }

    #[test]
    fn test_hook_output_sync_parses_permission_request_specific_output() {
        // PermissionRequest hook output carries a permission decision,
        // optional reason, updated_input/updated_permissions overrides,
        // plus interrupt/retry signals.
        let fixture = r#"{
            "hookSpecificOutput": {
                "hookEventName": "PermissionRequest",
                "permissionDecision": "allow",
                "permissionDecisionReason": "plugin approved",
                "updatedInput": {"command": "git status"},
                "updatedPermissions": {"rules": ["Bash(git *)"]},
                "interrupt": true,
                "retry": false
            }
        }"#;
        let actual: HookOutput = serde_json::from_str(fixture).unwrap();
        match actual {
            HookOutput::Sync(sync) => match sync.hook_specific_output {
                Some(HookSpecificOutput::PermissionRequest {
                    permission_decision,
                    permission_decision_reason,
                    updated_input,
                    updated_permissions,
                    interrupt,
                    retry,
                    ..
                }) => {
                    assert_eq!(permission_decision, Some(PermissionDecision::Allow));
                    assert_eq!(
                        permission_decision_reason.as_deref(),
                        Some("plugin approved")
                    );
                    assert_eq!(updated_input.unwrap()["command"], "git status");
                    assert_eq!(updated_permissions.unwrap()["rules"][0], "Bash(git *)");
                    assert_eq!(interrupt, Some(true));
                    assert_eq!(retry, Some(false));
                }
                other => panic!("expected PermissionRequest specific output, got {other:?}"),
            },
            other => panic!("expected Sync variant, got {other:?}"),
        }
    }

    #[test]
    fn test_hook_output_sync_parses_session_start_specific_output() {
        let fixture = r#"{
            "hookSpecificOutput": {
                "hookEventName": "SessionStart",
                "additionalContext": "loaded context",
                "watchPaths": ["/a", "/b"]
            }
        }"#;
        let actual: HookOutput = serde_json::from_str(fixture).unwrap();
        match actual {
            HookOutput::Sync(sync) => match sync.hook_specific_output {
                Some(HookSpecificOutput::SessionStart {
                    additional_context, watch_paths, ..
                }) => {
                    assert_eq!(additional_context.as_deref(), Some("loaded context"));
                    assert_eq!(
                        watch_paths,
                        Some(vec![PathBuf::from("/a"), PathBuf::from("/b")])
                    );
                }
                other => panic!("expected SessionStart specific output, got {other:?}"),
            },
            other => panic!("expected Sync variant, got {other:?}"),
        }
    }

    #[test]
    fn test_hook_output_sync_empty_object_is_valid() {
        let fixture = r#"{}"#;
        let actual: HookOutput = serde_json::from_str(fixture).unwrap();
        match actual {
            HookOutput::Sync(sync) => {
                assert_eq!(sync.should_continue, None);
                assert_eq!(sync.decision, None);
                assert!(sync.hook_specific_output.is_none());
            }
            other => panic!("expected Sync variant, got {other:?}"),
        }
    }

    // ---- Notification + Setup wire tests ----

    #[test]
    fn test_hook_input_serializes_notification_with_snake_case_fields() {
        let input = HookInput {
            base: sample_base("Notification"),
            payload: HookInputPayload::Notification {
                message: "OAuth complete".to_string(),
                title: Some("Authenticated".to_string()),
                notification_type: "auth_success".to_string(),
            },
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["hook_event_name"], "Notification");
        assert_eq!(json["message"], "OAuth complete");
        assert_eq!(json["title"], "Authenticated");
        assert_eq!(json["notification_type"], "auth_success");
    }

    #[test]
    fn test_hook_input_serializes_notification_omits_title_when_none() {
        let input = HookInput {
            base: sample_base("Notification"),
            payload: HookInputPayload::Notification {
                message: "idle".to_string(),
                title: None,
                notification_type: "idle_prompt".to_string(),
            },
        };
        let json = serde_json::to_value(&input).unwrap();
        assert!(json.get("title").is_none());
        assert_eq!(json["notification_type"], "idle_prompt");
    }

    #[test]
    fn test_hook_input_serializes_setup_with_trigger() {
        let input = HookInput {
            base: sample_base("Setup"),
            payload: HookInputPayload::Setup { trigger: "init".to_string() },
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["hook_event_name"], "Setup");
        assert_eq!(json["trigger"], "init");
    }

    // ---- ConfigChange wire tests ----

    #[test]
    fn test_hook_input_config_change_wire_format() {
        let input = HookInput {
            base: sample_base("ConfigChange"),
            payload: HookInputPayload::ConfigChange {
                source: "user_settings".to_string(),
                file_path: Some(PathBuf::from("/home/u/.forge/config.toml")),
            },
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["hook_event_name"], "ConfigChange");
        assert_eq!(json["source"], "user_settings");
        assert_eq!(json["file_path"], "/home/u/.forge/config.toml");
        // camelCase variant must NOT appear.
        assert!(json.get("filePath").is_none());
    }

    #[test]
    fn test_hook_input_config_change_omits_file_path_when_none() {
        let input = HookInput {
            base: sample_base("ConfigChange"),
            payload: HookInputPayload::ConfigChange {
                source: "plugins".to_string(),
                file_path: None,
            },
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["source"], "plugins");
        assert!(json.get("file_path").is_none());
    }

    // ---- Subagent wire tests ----

    #[test]
    fn test_hook_input_subagent_start_wire_format() {
        let input = HookInput {
            base: sample_base("SubagentStart"),
            payload: HookInputPayload::SubagentStart {
                agent_id: "sub-1".to_string(),
                agent_type: "muse".to_string(),
            },
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["hook_event_name"], "SubagentStart");
        assert_eq!(json["agent_id"], "sub-1");
        assert_eq!(json["agent_type"], "muse");
    }

    #[test]
    fn test_hook_input_subagent_stop_wire_format_uses_snake_case() {
        // All fields are snake_case on the wire — handled by enum-level
        // `rename_all = "snake_case"`.
        let input = HookInput {
            base: sample_base("SubagentStop"),
            payload: HookInputPayload::SubagentStop {
                agent_id: "sub-2".to_string(),
                agent_type: "forge".to_string(),
                agent_transcript_path: PathBuf::from("/tmp/sub-2.jsonl"),
                stop_hook_active: true,
                last_assistant_message: Some("ok".to_string()),
            },
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["hook_event_name"], "SubagentStop");
        assert_eq!(json["agent_id"], "sub-2");
        assert_eq!(json["agent_type"], "forge");
        assert_eq!(json["agent_transcript_path"], "/tmp/sub-2.jsonl");
        assert_eq!(json["stop_hook_active"], true);
        assert_eq!(json["last_assistant_message"], "ok");
        // camelCase variants must NOT appear on the wire.
        assert!(json.get("agentTranscriptPath").is_none());
        assert!(json.get("stopHookActive").is_none());
        assert!(json.get("lastAssistantMessage").is_none());
    }

    #[test]
    fn test_hook_input_subagent_stop_omits_last_assistant_message_when_none() {
        let input = HookInput {
            base: sample_base("SubagentStop"),
            payload: HookInputPayload::SubagentStop {
                agent_id: "sub-3".to_string(),
                agent_type: "sage".to_string(),
                agent_transcript_path: PathBuf::from("/tmp/sub-3.jsonl"),
                stop_hook_active: false,
                last_assistant_message: None,
            },
        };
        let json = serde_json::to_value(&input).unwrap();
        assert!(json.get("last_assistant_message").is_none());
    }

    // ---- Permission wire tests ----

    #[test]
    fn test_hook_input_permission_request_wire_format() {
        use crate::{PermissionBehavior, PermissionDestination, PermissionUpdate};
        let input = HookInput {
            base: sample_base("PermissionRequest"),
            payload: HookInputPayload::PermissionRequest {
                tool_name: "Bash".to_string(),
                tool_input: json!({"command": "git status"}),
                permission_suggestions: vec![PermissionUpdate {
                    rules: vec!["Bash(git *)".to_string()],
                    behavior: PermissionBehavior::Allow,
                    destination: PermissionDestination::ProjectSettings,
                }],
            },
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["hook_event_name"], "PermissionRequest");
        assert_eq!(json["tool_name"], "Bash");
        assert_eq!(json["tool_input"]["command"], "git status");
        // Field is snake_case on the wire.
        assert_eq!(json["permission_suggestions"][0]["behavior"], "allow");
        assert_eq!(
            json["permission_suggestions"][0]["destination"],
            "projectSettings"
        );
        assert!(json.get("permissionSuggestions").is_none());
    }

    #[test]
    fn test_hook_input_permission_denied_wire_format() {
        let input = HookInput {
            base: sample_base("PermissionDenied"),
            payload: HookInputPayload::PermissionDenied {
                tool_name: "Write".to_string(),
                tool_input: json!({"path": "/etc/passwd"}),
                tool_use_id: "toolu_99".to_string(),
                reason: "policy violation".to_string(),
            },
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["hook_event_name"], "PermissionDenied");
        assert_eq!(json["tool_name"], "Write");
        assert_eq!(json["tool_use_id"], "toolu_99");
        assert_eq!(json["reason"], "policy violation");
    }

    #[test]
    fn test_hook_input_cwd_changed_wire_format() {
        let input = HookInput {
            base: sample_base("CwdChanged"),
            payload: HookInputPayload::CwdChanged {
                old_cwd: PathBuf::from("/tmp/a"),
                new_cwd: PathBuf::from("/tmp/b"),
            },
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["hook_event_name"], "CwdChanged");
        assert_eq!(json["old_cwd"], "/tmp/a");
        assert_eq!(json["new_cwd"], "/tmp/b");
    }

    #[test]
    fn test_hook_input_file_changed_wire_format() {
        let input = HookInput {
            base: sample_base("FileChanged"),
            payload: HookInputPayload::FileChanged {
                file_path: PathBuf::from("/tmp/file.rs"),
                event: "change".to_string(),
            },
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["hook_event_name"], "FileChanged");
        assert_eq!(json["file_path"], "/tmp/file.rs");
        assert_eq!(json["event"], "change");
    }

    #[test]
    fn test_hook_input_worktree_create_wire_format() {
        let input = HookInput {
            base: sample_base("WorktreeCreate"),
            payload: HookInputPayload::WorktreeCreate { name: "feature-auth".to_string() },
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["hook_event_name"], "WorktreeCreate");
        assert_eq!(json["name"], "feature-auth");
    }

    #[test]
    fn test_hook_input_worktree_remove_wire_format() {
        let input = HookInput {
            base: sample_base("WorktreeRemove"),
            payload: HookInputPayload::WorktreeRemove {
                worktree_path: PathBuf::from("/tmp/wt/feature"),
            },
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["hook_event_name"], "WorktreeRemove");
        assert_eq!(json["worktree_path"], "/tmp/wt/feature");
    }

    /// Parsing a `WorktreeCreate` hook's JSON stdout
    /// should surface the `worktreePath` field on the specific-output
    /// variant. Mirrors Claude Code's wire format
    /// (`claude-code/src/utils/hooks.ts:4956`) where a `command`-type
    /// hook can hand the CLI a custom path to skip the built-in
    /// `git worktree add` fallback.
    ///
    /// The plain-text fallback ("hook stdout is just `/path/to/wt`") is
    /// handled one layer up in
    /// [`crate::AggregatedHookResult::merge`], which folds non-JSON
    /// stdout into `additional_contexts` — so there is no plain-text
    /// branch at the `HookOutput` parser level. Plugins that want the
    /// override behaviour must emit the full JSON envelope.
    #[test]
    fn test_hook_output_parses_worktree_create_specific_output() {
        // Case 1: full JSON envelope with an explicit worktreePath.
        let fixture_with_path = r#"{
            "hookSpecificOutput": {
                "hookEventName": "WorktreeCreate",
                "worktreePath": "/tmp/wt/override"
            }
        }"#;
        let actual: HookOutput = serde_json::from_str(fixture_with_path).unwrap();
        match actual {
            HookOutput::Sync(sync) => match sync.hook_specific_output {
                Some(HookSpecificOutput::WorktreeCreate { worktree_path }) => {
                    assert_eq!(worktree_path, Some(PathBuf::from("/tmp/wt/override")));
                }
                other => panic!("expected WorktreeCreate specific output, got {other:?}"),
            },
            other => panic!("expected Sync variant, got {other:?}"),
        }

        // Case 2: JSON envelope without a worktreePath — the field is
        // optional (`#[serde(default)]`) so a bare
        // `{ "hookEventName": "WorktreeCreate" }` parses cleanly and
        // the field defaults to `None`. This mirrors how plain-text
        // hooks that `echo` status without overriding the path are
        // treated upstream in `AggregatedHookResult::merge`.
        let fixture_without_path = r#"{
            "hookSpecificOutput": {
                "hookEventName": "WorktreeCreate"
            }
        }"#;
        let actual: HookOutput = serde_json::from_str(fixture_without_path).unwrap();
        match actual {
            HookOutput::Sync(sync) => match sync.hook_specific_output {
                Some(HookSpecificOutput::WorktreeCreate { worktree_path }) => {
                    assert_eq!(worktree_path, None);
                }
                other => panic!("expected WorktreeCreate specific output, got {other:?}"),
            },
            other => panic!("expected Sync variant, got {other:?}"),
        }
    }

    // ---- InstructionsLoaded wire test ----

    #[test]
    fn test_hook_input_instructions_loaded_wire_format() {
        let input = HookInput {
            base: sample_base("InstructionsLoaded"),
            payload: HookInputPayload::InstructionsLoaded {
                file_path: PathBuf::from("/repo/AGENTS.md"),
                memory_type: "project".to_string(),
                load_reason: "session_start".to_string(),
                globs: Some(vec!["**/*.rs".to_string()]),
                trigger_file_path: None,
                parent_file_path: None,
            },
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["hook_event_name"], "InstructionsLoaded");
        assert_eq!(json["file_path"], "/repo/AGENTS.md");
        assert_eq!(json["memory_type"], "project");
        assert_eq!(json["load_reason"], "session_start");
        assert_eq!(json["globs"][0], "**/*.rs");
        // None optional fields are omitted.
        assert!(json.get("trigger_file_path").is_none());
        assert!(json.get("parent_file_path").is_none());
    }

    // ---- Elicitation + ElicitationResult wire tests ----

    #[test]
    fn test_hook_input_elicitation_wire_format() {
        let input = HookInput {
            base: sample_base("Elicitation"),
            payload: HookInputPayload::Elicitation {
                server_name: "github".to_string(),
                message: "Provide a PR title".to_string(),
                requested_schema: Some(json!({
                    "type": "object",
                    "properties": {"title": {"type": "string"}}
                })),
                mode: Some("form".to_string()),
                url: None,
                elicitation_id: None,
            },
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["hook_event_name"], "Elicitation");
        assert_eq!(json["mcp_server_name"], "github");
        assert_eq!(json["message"], "Provide a PR title");
        assert_eq!(json["requested_schema"]["type"], "object");
        assert_eq!(json["mode"], "form");
        // The old camelCase alias must NOT appear on the wire.
        assert!(json.get("serverName").is_none());
        assert!(json.get("server_name").is_none());
        assert!(json.get("requestedSchema").is_none());
        // url is None and must be omitted.
        assert!(json.get("url").is_none());
    }

    #[test]
    fn test_hook_input_elicitation_result_wire_format() {
        let input = HookInput {
            base: sample_base("ElicitationResult"),
            payload: HookInputPayload::ElicitationResult {
                server_name: "github".to_string(),
                action: "accept".to_string(),
                content: Some(json!({"title": "My PR"})),
                mode: None,
                elicitation_id: None,
            },
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["hook_event_name"], "ElicitationResult");
        assert_eq!(json["mcp_server_name"], "github");
        assert_eq!(json["action"], "accept");
        assert_eq!(json["content"]["title"], "My PR");
        // The old camelCase alias must NOT appear on the wire.
        assert!(json.get("serverName").is_none());
        assert!(json.get("server_name").is_none());
    }

    #[test]
    fn test_hook_input_elicitation_result_includes_mode() {
        let input = HookInput {
            base: sample_base("ElicitationResult"),
            payload: HookInputPayload::ElicitationResult {
                server_name: "github".to_string(),
                action: "accept".to_string(),
                content: None,
                mode: Some("form".to_string()),
                elicitation_id: None,
            },
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["mode"], "form");
    }
}
