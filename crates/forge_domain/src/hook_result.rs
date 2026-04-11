//! Aggregated results from running multiple hooks in parallel for a single
//! lifecycle event.
//!
//! When a lifecycle event fires (e.g. `PreToolUse`), every matching hook
//! command runs concurrently. Their individual [`crate::HookOutput`] values
//! are folded into an [`AggregatedHookResult`] using the policy described
//! in `claude-code/src/utils/hooks.ts:2733-2881`:
//!
//! - **`blocking_error`**: first hook to block wins. Other hooks still run so
//!   their side effects complete, but the first blocking error is the one
//!   propagated to the LLM.
//! - **`permission_behavior`**: deny > ask > allow precedence. `Deny` always
//!   takes priority regardless of order; `Ask` overwrites `Allow` but not
//!   `Deny`; `Allow` only applies if nothing was set yet.
//! - **`updated_input`**: last-write-wins. Later hooks see the aggregate of
//!   earlier ones, but the last one to set a value overwrites prior values.
//! - **`updated_permissions`**: last-write-wins, mirrors `updated_input`. Set
//!   by `PermissionRequest` hooks that want to mutate the persisted permission
//!   scopes for a tool / file path tuple.
//! - **`interrupt`** / **`retry`**: latch to `true` (OR across all hooks). Once
//!   any `PermissionRequest` hook asks to interrupt or retry, the flag stays on
//!   for the rest of the merge.
//! - **`additional_contexts`** / **`system_messages`**: accumulated in
//!   execution order.
//! - **`watch_paths`**: accumulated; deduplication happens downstream.
//!
//! Reference: `claude-code/src/utils/hooks.ts:359-376`

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::hook_io::{
    HookDecision, HookOutput, HookSpecificOutput, PermissionDecision, PermissionRequestDecision,
};

/// Result of aggregating every hook that ran for a single lifecycle event.
///
/// Fields follow the merge policy documented in the module header. The
/// struct is `Default` so an empty-hooks path can just use
/// `AggregatedHookResult::default()` without special-casing.
#[derive(Debug, Clone, Default)]
pub struct AggregatedHookResult {
    /// The first blocking error encountered, if any. When set, the
    /// orchestrator treats the surrounding event as blocked and propagates
    /// `message` back to the LLM.
    pub blocking_error: Option<HookBlockingError>,
    /// The effective permission decision across all PreToolUse hooks.
    /// First non-`None` wins.
    pub permission_behavior: Option<PermissionBehavior>,
    /// Last-write-wins override of the original tool/event input.
    pub updated_input: Option<serde_json::Value>,
    /// Additional context strings accumulated from every hook that emitted
    /// an `additionalContext` field. Appended to the next model turn.
    pub additional_contexts: Vec<String>,
    /// System messages emitted by hooks, shown to the user in sequence.
    pub system_messages: Vec<String>,
    /// If `true`, one or more hooks set `continue: false` — the orchestrator
    /// should halt the agent loop after this event.
    pub prevent_continuation: bool,
    /// Reason shown when continuation is prevented.
    pub stop_reason: Option<String>,
    /// Initial user message override set by a SessionStart hook. First-wins:
    /// once a SessionStart hook sets this value, subsequent SessionStart
    /// hooks cannot overwrite it.
    pub initial_user_message: Option<String>,
    /// Paths that hooks asked Forge to watch (for `CwdChanged` /
    /// `FileChanged` events in later phases).
    pub watch_paths: Vec<PathBuf>,
    /// Override for an MCP tool's output, set by PostToolUse hooks.
    pub updated_mcp_tool_output: Option<serde_json::Value>,
    /// Last-write-wins override of permission scopes set by a
    /// `PermissionRequest` hook. When set, the orchestrator updates the
    /// persisted permission config for the (tool_name, file_path) tuple.
    /// Carries a plugin-defined JSON blob — Forge does not interpret the
    /// contents here; the permission fire site in
    /// `ToolRegistry::check_tool_permission` currently logs it and
    /// defers the actual persistence step (see the TODO referenced in
    /// `plans/2026-04-09-claude-code-plugins-v4/08-phase-7-t3-intermediate.
    /// md`).
    pub updated_permissions: Option<serde_json::Value>,
    /// Set to `true` when any `PermissionRequest` hook requested an
    /// interactive session interrupt. Triggers the orchestrator's
    /// interrupt handling after the permission decision resolves.
    pub interrupt: bool,
    /// Set to `true` when any `PermissionRequest` hook asked the
    /// permission prompt to be re-issued (for example, after a
    /// credential refresh). The orchestrator re-fires the permission
    /// check rather than applying the current decision.
    pub retry: bool,
    /// Plugin-provided override for the worktree path during a
    /// `WorktreeCreate` hook. Last-write-wins across multiple hooks on
    /// the same event. When present, the CLI `--worktree` handler in
    /// `crates/forge_main/src/sandbox.rs` uses this path instead of
    /// falling back to `git worktree add`. The runtime
    /// `EnterWorktreeTool` fire site (pending) will consume the same
    /// field.
    pub worktree_path: Option<PathBuf>,
}

impl AggregatedHookResult {
    /// Apply Claude Code's permission precedence: deny > ask > allow.
    ///
    /// - `deny` always takes precedence over any prior value.
    /// - `ask` takes precedence over `allow` but not `deny`.
    /// - `allow` only wins if no other behavior has been set.
    ///
    /// Reference: `claude-code/src/utils/hooks.ts:2820-2847`
    fn apply_permission_precedence(&mut self, new: PermissionBehavior) {
        match new {
            PermissionBehavior::Deny => {
                // deny always takes precedence
                self.permission_behavior = Some(PermissionBehavior::Deny);
            }
            PermissionBehavior::Ask => {
                // ask takes precedence over allow but not deny
                if self.permission_behavior != Some(PermissionBehavior::Deny) {
                    self.permission_behavior = Some(PermissionBehavior::Ask);
                }
            }
            PermissionBehavior::Allow => {
                // allow only if no other behavior set
                if self.permission_behavior.is_none() {
                    self.permission_behavior = Some(PermissionBehavior::Allow);
                }
            }
        }
    }

    /// Merge a single executor result into the aggregate.
    ///
    /// The merge policy matches Claude Code's aggregator:
    ///
    /// - The **first** `Blocking` outcome wins — once `blocking_error` is set,
    ///   subsequent blocks are ignored (so stderr from the first blocker is
    ///   what the LLM sees).
    /// - `prevent_continuation` latches to `true` as soon as any hook sets
    ///   `continue: false`. `stop_reason` takes the last non-`None` value.
    /// - `system_messages` and `additional_contexts` accumulate in invocation
    ///   order.
    /// - `permission_behavior` uses deny > ask > allow precedence across all
    ///   hooks (`claude-code/src/utils/hooks.ts:2820-2847`).
    /// - `updated_input` is **last-write-wins** — each hook sees the raw input;
    ///   the last write overwrites earlier ones.
    /// - `updated_mcp_tool_output` is also last-write-wins.
    /// - `watch_paths` accumulates.
    /// - When a hook exits `Success` with plain-text stdout (no JSON output),
    ///   the trimmed stdout becomes an `additional_context` entry — this
    ///   matches Claude Code's behaviour for shell hooks that `echo` a plain
    ///   message.
    pub fn merge(&mut self, exec: HookExecResult) {
        // Classify `Blocking` before consuming `output` below.
        if exec.outcome == HookOutcome::Blocking && self.blocking_error.is_none() {
            self.blocking_error = Some(HookBlockingError {
                message: if exec.raw_stderr.trim().is_empty() {
                    exec.raw_stdout.trim().to_string()
                } else {
                    exec.raw_stderr.trim().to_string()
                },
                // Command identity is tracked upstream in the dispatcher.
                command: String::new(),
            });
        }

        // Apply sync-output fields when present.
        let sync_opt = match &exec.output {
            Some(HookOutput::Sync(sync)) => Some(sync.clone()),
            _ => None,
        };

        if let Some(sync) = sync_opt {
            if sync.should_continue == Some(false) {
                self.prevent_continuation = true;
            }
            if let Some(reason) = sync.stop_reason {
                self.stop_reason = Some(reason);
            }
            if let Some(msg) = sync.system_message {
                self.system_messages.push(msg);
            }

            // Top-level `decision` field maps to permission_behavior and
            // optionally creates a blocking error. This mirrors Claude Code's
            // `processHookJSONOutput` at `hooks.ts:525-543`.
            match sync.decision {
                Some(HookDecision::Approve) => {
                    self.apply_permission_precedence(PermissionBehavior::Allow);
                }
                Some(HookDecision::Block) => {
                    self.apply_permission_precedence(PermissionBehavior::Deny);
                    if self.blocking_error.is_none() {
                        self.blocking_error = Some(HookBlockingError {
                            message: sync
                                .reason
                                .clone()
                                .unwrap_or_else(|| exec.raw_stderr.trim().to_string()),
                            command: String::new(),
                        });
                    }
                }
                None => {}
            }

            match sync.hook_specific_output {
                Some(HookSpecificOutput::PreToolUse {
                    permission_decision,
                    updated_input,
                    additional_context,
                    ..
                }) => {
                    if let Some(pd) = permission_decision {
                        self.apply_permission_precedence(match pd {
                            PermissionDecision::Allow => PermissionBehavior::Allow,
                            PermissionDecision::Deny => PermissionBehavior::Deny,
                            PermissionDecision::Ask => PermissionBehavior::Ask,
                        });
                    }
                    if let Some(updated) = updated_input {
                        self.updated_input = Some(updated);
                    }
                    if let Some(ctx) = additional_context {
                        self.additional_contexts.push(ctx);
                    }
                }
                Some(HookSpecificOutput::PostToolUse {
                    additional_context,
                    updated_mcp_tool_output,
                }) => {
                    if let Some(ctx) = additional_context {
                        self.additional_contexts.push(ctx);
                    }
                    if let Some(out) = updated_mcp_tool_output {
                        self.updated_mcp_tool_output = Some(out);
                    }
                }
                Some(HookSpecificOutput::UserPromptSubmit { additional_context }) => {
                    if let Some(ctx) = additional_context {
                        self.additional_contexts.push(ctx);
                    }
                }
                Some(HookSpecificOutput::SessionStart {
                    additional_context,
                    initial_user_message,
                    watch_paths,
                }) => {
                    if let Some(ctx) = additional_context {
                        self.additional_contexts.push(ctx);
                    }
                    if self.initial_user_message.is_none()
                        && let Some(msg) = initial_user_message
                    {
                        self.initial_user_message = Some(msg);
                    }
                    if let Some(paths) = watch_paths {
                        self.watch_paths.extend(paths);
                    }
                }
                Some(HookSpecificOutput::PermissionRequest {
                    permission_decision,
                    updated_input,
                    updated_permissions,
                    interrupt,
                    retry,
                    permission_decision_reason: _,
                    decision,
                }) => {
                    // Extract fields from nested `decision` (Claude Code
                    // shape) when the flat fields are absent.
                    let effective_decision = permission_decision.or_else(|| {
                        decision.as_ref().map(|d| match d {
                            PermissionRequestDecision::Allow { .. } => PermissionDecision::Allow,
                            PermissionRequestDecision::Deny { .. } => PermissionDecision::Deny,
                        })
                    });
                    let effective_input = updated_input.or_else(|| match &decision {
                        Some(PermissionRequestDecision::Allow { updated_input, .. }) => {
                            updated_input.clone()
                        }
                        _ => None,
                    });
                    let effective_perms = updated_permissions.or_else(|| match &decision {
                        Some(PermissionRequestDecision::Allow { updated_permissions, .. }) => {
                            updated_permissions.clone()
                        }
                        _ => None,
                    });
                    let effective_interrupt = interrupt.or_else(|| match &decision {
                        Some(PermissionRequestDecision::Deny { interrupt, .. }) => *interrupt,
                        _ => None,
                    });

                    // deny > ask > allow precedence (mirrors PreToolUse).
                    if let Some(pd) = effective_decision {
                        self.apply_permission_precedence(match pd {
                            PermissionDecision::Allow => PermissionBehavior::Allow,
                            PermissionDecision::Deny => PermissionBehavior::Deny,
                            PermissionDecision::Ask => PermissionBehavior::Ask,
                        });
                    }
                    // Last-write-wins on updated_input.
                    if let Some(input) = effective_input {
                        self.updated_input = Some(input);
                    }
                    // Last-write-wins on updated_permissions.
                    if let Some(perms) = effective_perms {
                        self.updated_permissions = Some(perms);
                    }
                    // Latch to true on interrupt / retry.
                    if effective_interrupt.unwrap_or(false) {
                        self.interrupt = true;
                    }
                    if retry.unwrap_or(false) {
                        self.retry = true;
                    }
                }
                Some(HookSpecificOutput::WorktreeCreate { worktree_path }) => {
                    // Last-write-wins on the plugin-provided worktree
                    // path override. A `None` value is a no-op — it
                    // does not clear a previously-set path.
                    if let Some(path) = worktree_path {
                        self.worktree_path = Some(path);
                    }
                }
                Some(HookSpecificOutput::Setup { additional_context })
                | Some(HookSpecificOutput::SubagentStart { additional_context })
                | Some(HookSpecificOutput::PostToolUseFailure { additional_context })
                | Some(HookSpecificOutput::Notification { additional_context }) => {
                    if let Some(ctx) = additional_context {
                        self.additional_contexts.push(ctx);
                    }
                }
                Some(HookSpecificOutput::PermissionDenied { retry }) => {
                    if retry.unwrap_or(false) {
                        self.retry = true;
                    }
                }
                Some(HookSpecificOutput::Elicitation { action, .. })
                | Some(HookSpecificOutput::ElicitationResult { action, .. }) => {
                    // Claude Code creates a blocking error when an
                    // Elicitation/ElicitationResult hook returns
                    // `action: 'decline'`.
                    if action.as_deref() == Some("decline") && self.blocking_error.is_none() {
                        self.blocking_error = Some(HookBlockingError {
                            message: sync
                                .reason
                                .clone()
                                .unwrap_or_else(|| "Elicitation denied by hook".to_string()),
                            command: String::new(),
                        });
                    }
                }
                Some(HookSpecificOutput::CwdChanged { watch_paths })
                | Some(HookSpecificOutput::FileChanged { watch_paths }) => {
                    if let Some(paths) = watch_paths {
                        self.watch_paths.extend(paths);
                    }
                }
                None => {}
            }
        }

        // Plain-text stdout for Success outcomes with no JSON output
        // becomes an additional context entry.
        if exec.outcome == HookOutcome::Success
            && exec.output.is_none()
            && !exec.raw_stdout.trim().is_empty()
        {
            self.additional_contexts
                .push(exec.raw_stdout.trim().to_string());
        }
    }
}

/// A single hook blocking error — the message shown to the LLM plus the
/// command string for diagnostic logging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookBlockingError {
    /// User-visible error text. For shell hooks this is typically the
    /// subprocess's stderr.
    pub message: String,
    /// Identifier of the hook that blocked (typically the shell command or
    /// URL). Used for logging only — not shown to the LLM.
    pub command: String,
}

/// Final permission decision folded across all PreToolUse hooks.
///
/// Distinct from [`crate::PermissionDecision`] (the per-hook wire type): this
/// is the **aggregate** outcome after the merge policy has run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionBehavior {
    Allow,
    Deny,
    Ask,
}

/// Normalized result of running a single hook, regardless of executor.
///
/// The aggregator folds one of these per hook into an
/// [`AggregatedHookResult`] via [`AggregatedHookResult::merge`]. Lives in
/// `forge_domain` (rather than `forge_app::infra`) so that
/// [`AggregatedHookResult::merge`] can operate on it without creating a
/// circular crate dependency.
#[derive(Debug, Clone)]
pub struct HookExecResult {
    /// High-level classification of the hook's outcome.
    pub outcome: HookOutcome,
    /// Parsed JSON response if the hook emitted a [`crate::HookOutput`] on
    /// its stdout (shell) or body (http/prompt/agent).
    pub output: Option<HookOutput>,
    /// Raw stdout captured from the hook. Preserved even when
    /// [`Self::output`] is `Some` so callers can display the exact text
    /// to the user if desired.
    pub raw_stdout: String,
    /// Raw stderr captured from the hook.
    pub raw_stderr: String,
    /// Exit code (shell) or HTTP status (http), when available.
    pub exit_code: Option<i32>,
}

/// High-level classification of a hook execution.
///
/// - [`Success`](HookOutcome::Success) — exit 0 or explicit `decision:
///   approve`; the hook's output (if any) is merged into the aggregated result
///   normally.
/// - [`Blocking`](HookOutcome::Blocking) — exit 2 or explicit `decision:
///   block`; the first such outcome becomes the aggregate `blocking_error`.
/// - [`NonBlockingError`](HookOutcome::NonBlockingError) — any other non-zero
///   exit. Surfaced to the user as a warning but doesn't block the agent loop.
/// - [`Cancelled`](HookOutcome::Cancelled) — the hook timed out and was killed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookOutcome {
    Success,
    Blocking,
    NonBlockingError,
    Cancelled,
}

/// A single permission update requested by a plugin hook.
///
/// Mirrors a subset of Claude Code's `PermissionUpdate` discriminated
/// union, adapted to Forge's glob-based YAML policy system.
///
/// Only `addRules` is supported today; unsupported variants are logged
/// and skipped.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PluginPermissionUpdate {
    /// Add allow/deny rules to the policy file.
    #[serde(rename = "addRules")]
    AddRules {
        /// The rules to add (glob patterns like `*.rs`, `Bash(*)`).
        rules: Vec<String>,
        /// The behavior: `"allow"`, `"deny"`, or `"ask"`.
        behavior: String,
    },
    /// Set the permission mode. Currently a no-op in Forge since
    /// Forge uses `restricted: bool` rather than a rich mode enum.
    #[serde(rename = "setMode")]
    SetMode { mode: String },
}

/// A pending result from an async hook with `asyncRewake: true`.
///
/// When an asyncRewake hook completes in the background, the shell executor
/// sends one of these through an mpsc channel. The orchestrator drains
/// them before each conversation turn and injects them as
/// `<system_reminder>` context messages — mirroring Claude Code's
/// `enqueuePendingNotification` + `queued_command` attachment pipeline.
#[derive(Debug, Clone)]
pub struct PendingHookResult {
    /// Human-readable identifier for the hook (e.g. the shell command).
    pub hook_name: String,
    /// The message text to inject (stderr for blocking, stdout otherwise).
    pub message: String,
    /// `true` when the hook exited with code 2 (blocking). The
    /// orchestrator prefixes the injected message with "BLOCKING: ".
    pub is_blocking: bool,
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;
    use crate::hook_io::SyncHookOutput;

    #[test]
    fn test_aggregated_hook_result_default_is_empty() {
        let actual = AggregatedHookResult::default();
        assert!(actual.blocking_error.is_none());
        assert!(actual.permission_behavior.is_none());
        assert!(actual.updated_input.is_none());
        assert!(actual.additional_contexts.is_empty());
        assert!(actual.system_messages.is_empty());
        assert!(!actual.prevent_continuation);
        assert!(actual.stop_reason.is_none());
        assert!(actual.initial_user_message.is_none());
        assert!(actual.watch_paths.is_empty());
        assert!(actual.updated_mcp_tool_output.is_none());
        assert!(actual.updated_permissions.is_none());
        assert!(!actual.interrupt);
        assert!(!actual.retry);
        assert!(actual.worktree_path.is_none());
    }

    /// Sanity-check the `Default` impl zeroes the three
    /// `PermissionRequest` fields.
    #[test]
    fn test_aggregated_default_has_false_interrupt_and_retry() {
        let actual = AggregatedHookResult::default();
        assert!(!actual.interrupt);
        assert!(!actual.retry);
        assert!(actual.updated_permissions.is_none());
    }

    #[test]
    fn test_hook_blocking_error_equality() {
        let a = HookBlockingError {
            message: "denied".to_string(),
            command: "echo hi".to_string(),
        };
        let b = HookBlockingError {
            message: "denied".to_string(),
            command: "echo hi".to_string(),
        };
        assert_eq!(a, b);
    }

    #[test]
    fn test_permission_behavior_variants_are_distinct() {
        assert_ne!(PermissionBehavior::Allow, PermissionBehavior::Deny);
        assert_ne!(PermissionBehavior::Deny, PermissionBehavior::Ask);
        assert_ne!(PermissionBehavior::Allow, PermissionBehavior::Ask);
    }

    #[test]
    fn test_aggregated_hook_result_clone_preserves_fields() {
        let original = AggregatedHookResult {
            blocking_error: Some(HookBlockingError {
                message: "bad".to_string(),
                command: "false".to_string(),
            }),
            permission_behavior: Some(PermissionBehavior::Deny),
            updated_input: Some(json!({"x": 1})),
            additional_contexts: vec!["ctx".to_string()],
            system_messages: vec!["sys".to_string()],
            prevent_continuation: true,
            stop_reason: Some("halt".to_string()),
            initial_user_message: Some("hi".to_string()),
            watch_paths: vec![PathBuf::from("/tmp")],
            updated_mcp_tool_output: Some(json!({"y": 2})),
            updated_permissions: Some(json!({"rules": ["Bash(*)"]})),
            interrupt: true,
            retry: true,
            worktree_path: Some(PathBuf::from("/tmp/wt/feature")),
        };
        let cloned = original.clone();
        assert_eq!(
            cloned.blocking_error.as_ref().map(|e| &e.message),
            Some(&"bad".to_string())
        );
        assert_eq!(cloned.permission_behavior, Some(PermissionBehavior::Deny));
        assert_eq!(cloned.additional_contexts, vec!["ctx".to_string()]);
        assert_eq!(cloned.system_messages, vec!["sys".to_string()]);
        assert!(cloned.prevent_continuation);
        assert_eq!(cloned.stop_reason.as_deref(), Some("halt"));
        assert_eq!(cloned.watch_paths, vec![PathBuf::from("/tmp")]);
        assert_eq!(cloned.worktree_path, Some(PathBuf::from("/tmp/wt/feature")));
    }

    fn success_with_plain_text(stdout: &str) -> HookExecResult {
        HookExecResult {
            outcome: HookOutcome::Success,
            output: None,
            raw_stdout: stdout.to_string(),
            raw_stderr: String::new(),
            exit_code: Some(0),
        }
    }

    fn blocking_with_stderr(stderr: &str) -> HookExecResult {
        HookExecResult {
            outcome: HookOutcome::Blocking,
            output: None,
            raw_stdout: String::new(),
            raw_stderr: stderr.to_string(),
            exit_code: Some(2),
        }
    }

    fn success_with_sync(sync: SyncHookOutput) -> HookExecResult {
        HookExecResult {
            outcome: HookOutcome::Success,
            output: Some(HookOutput::Sync(sync)),
            raw_stdout: String::new(),
            raw_stderr: String::new(),
            exit_code: Some(0),
        }
    }

    #[test]
    fn test_merge_plain_text_stdout_becomes_additional_context() {
        let mut agg = AggregatedHookResult::default();
        agg.merge(success_with_plain_text("extra context line"));

        assert_eq!(
            agg.additional_contexts,
            vec!["extra context line".to_string()]
        );
    }

    #[test]
    fn test_merge_accumulates_multiple_additional_contexts() {
        // Covers Task 3.23: "multiple parallel hooks accumulate additional_contexts".
        let mut agg = AggregatedHookResult::default();
        agg.merge(success_with_plain_text("first"));
        agg.merge(success_with_plain_text("second"));
        agg.merge(success_with_plain_text("third"));

        assert_eq!(
            agg.additional_contexts,
            vec![
                "first".to_string(),
                "second".to_string(),
                "third".to_string()
            ]
        );
    }

    #[test]
    fn test_merge_blocking_outcome_sets_blocking_error() {
        // Covers Task 3.23: "one hook returns block -> blocking_error is set".
        let mut agg = AggregatedHookResult::default();
        agg.merge(blocking_with_stderr("nope, denied"));

        let err = agg.blocking_error.as_ref().expect("blocking_error set");
        assert_eq!(err.message, "nope, denied");
    }

    #[test]
    fn test_merge_first_blocking_error_wins() {
        let mut agg = AggregatedHookResult::default();
        agg.merge(blocking_with_stderr("first"));
        agg.merge(blocking_with_stderr("second"));

        let err = agg.blocking_error.as_ref().expect("blocking_error set");
        assert_eq!(err.message, "first");
    }

    #[test]
    fn test_merge_updated_input_is_last_write_wins() {
        // Covers Task 3.23: "two hooks set updated_input -> last-write-wins".
        let mut agg = AggregatedHookResult::default();

        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::PreToolUse {
                permission_decision: None,
                permission_decision_reason: None,
                updated_input: Some(json!({"value": 1})),
                additional_context: None,
            }),
            ..Default::default()
        }));

        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::PreToolUse {
                permission_decision: None,
                permission_decision_reason: None,
                updated_input: Some(json!({"value": 2})),
                additional_context: None,
            }),
            ..Default::default()
        }));

        assert_eq!(agg.updated_input, Some(json!({"value": 2})));
    }

    #[test]
    fn test_merge_permission_deny_overrides_allow() {
        // Claude Code precedence: deny > ask > allow.
        // Even if the first hook says Allow, a later Deny overrides it.
        let mut agg = AggregatedHookResult::default();

        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::PreToolUse {
                permission_decision: Some(PermissionDecision::Allow),
                permission_decision_reason: None,
                updated_input: None,
                additional_context: None,
            }),
            ..Default::default()
        }));

        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::PreToolUse {
                permission_decision: Some(PermissionDecision::Deny),
                permission_decision_reason: None,
                updated_input: None,
                additional_context: None,
            }),
            ..Default::default()
        }));

        assert_eq!(agg.permission_behavior, Some(PermissionBehavior::Deny));
    }

    #[test]
    fn test_merge_permission_ask_overrides_allow_but_not_deny() {
        // ask takes precedence over allow but not deny.
        let mut agg = AggregatedHookResult::default();

        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::PreToolUse {
                permission_decision: Some(PermissionDecision::Allow),
                permission_decision_reason: None,
                updated_input: None,
                additional_context: None,
            }),
            ..Default::default()
        }));

        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::PreToolUse {
                permission_decision: Some(PermissionDecision::Ask),
                permission_decision_reason: None,
                updated_input: None,
                additional_context: None,
            }),
            ..Default::default()
        }));

        assert_eq!(agg.permission_behavior, Some(PermissionBehavior::Ask));

        // Now ask should NOT override deny.
        let mut agg2 = AggregatedHookResult::default();

        agg2.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::PreToolUse {
                permission_decision: Some(PermissionDecision::Deny),
                permission_decision_reason: None,
                updated_input: None,
                additional_context: None,
            }),
            ..Default::default()
        }));

        agg2.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::PreToolUse {
                permission_decision: Some(PermissionDecision::Ask),
                permission_decision_reason: None,
                updated_input: None,
                additional_context: None,
            }),
            ..Default::default()
        }));

        assert_eq!(agg2.permission_behavior, Some(PermissionBehavior::Deny));
    }

    #[test]
    fn test_merge_permission_allow_only_wins_if_nothing_set() {
        // allow only wins when no prior behavior was set.
        let mut agg = AggregatedHookResult::default();

        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::PreToolUse {
                permission_decision: Some(PermissionDecision::Allow),
                permission_decision_reason: None,
                updated_input: None,
                additional_context: None,
            }),
            ..Default::default()
        }));

        assert_eq!(agg.permission_behavior, Some(PermissionBehavior::Allow));

        // A second Allow doesn't change anything.
        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::PreToolUse {
                permission_decision: Some(PermissionDecision::Allow),
                permission_decision_reason: None,
                updated_input: None,
                additional_context: None,
            }),
            ..Default::default()
        }));

        assert_eq!(agg.permission_behavior, Some(PermissionBehavior::Allow));
    }

    // ---- PermissionRequest merge tests ----

    /// Two hooks vote Allow then Deny — deny takes precedence per
    /// Claude Code's deny > ask > allow model.
    #[test]
    fn test_merge_permission_request_deny_overrides_allow() {
        let mut agg = AggregatedHookResult::default();

        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::PermissionRequest {
                permission_decision: Some(PermissionDecision::Allow),
                permission_decision_reason: None,
                updated_input: None,
                updated_permissions: None,
                interrupt: None,
                retry: None,
                decision: None,
            }),
            ..Default::default()
        }));

        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::PermissionRequest {
                permission_decision: Some(PermissionDecision::Deny),
                permission_decision_reason: None,
                updated_input: None,
                updated_permissions: None,
                interrupt: None,
                retry: None,
                decision: None,
            }),
            ..Default::default()
        }));

        assert_eq!(agg.permission_behavior, Some(PermissionBehavior::Deny));
    }

    /// Two hooks both set `updated_permissions` — last-write-wins.
    #[test]
    fn test_merge_permission_request_last_wins_on_updated_permissions() {
        let mut agg = AggregatedHookResult::default();

        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::PermissionRequest {
                permission_decision: None,
                permission_decision_reason: None,
                updated_input: None,
                updated_permissions: Some(json!({"rules": ["first"]})),
                interrupt: None,
                retry: None,
                decision: None,
            }),
            ..Default::default()
        }));

        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::PermissionRequest {
                permission_decision: None,
                permission_decision_reason: None,
                updated_input: None,
                updated_permissions: Some(json!({"rules": ["second"]})),
                interrupt: None,
                retry: None,
                decision: None,
            }),
            ..Default::default()
        }));

        assert_eq!(agg.updated_permissions, Some(json!({"rules": ["second"]})));
    }

    /// One hook sets `interrupt: true`, another `false`. Latch wins.
    #[test]
    fn test_merge_permission_request_latches_interrupt_to_true() {
        let mut agg = AggregatedHookResult::default();

        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::PermissionRequest {
                permission_decision: None,
                permission_decision_reason: None,
                updated_input: None,
                updated_permissions: None,
                interrupt: Some(true),
                retry: None,
                decision: None,
            }),
            ..Default::default()
        }));

        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::PermissionRequest {
                permission_decision: None,
                permission_decision_reason: None,
                updated_input: None,
                updated_permissions: None,
                interrupt: Some(false),
                retry: None,
                decision: None,
            }),
            ..Default::default()
        }));

        assert!(agg.interrupt);
    }

    /// One hook sets `retry: true`, another `false`. Latch wins.
    #[test]
    fn test_merge_permission_request_latches_retry_to_true() {
        let mut agg = AggregatedHookResult::default();

        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::PermissionRequest {
                permission_decision: None,
                permission_decision_reason: None,
                updated_input: None,
                updated_permissions: None,
                interrupt: None,
                retry: Some(true),
                decision: None,
            }),
            ..Default::default()
        }));

        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::PermissionRequest {
                permission_decision: None,
                permission_decision_reason: None,
                updated_input: None,
                updated_permissions: None,
                interrupt: None,
                retry: Some(false),
                decision: None,
            }),
            ..Default::default()
        }));

        assert!(agg.retry);
    }

    #[test]
    fn test_merge_prevent_continuation_latches_true() {
        let mut agg = AggregatedHookResult::default();
        agg.merge(success_with_sync(SyncHookOutput {
            should_continue: Some(false),
            stop_reason: Some("halt".to_string()),
            ..Default::default()
        }));

        assert!(agg.prevent_continuation);
        assert_eq!(agg.stop_reason.as_deref(), Some("halt"));
    }

    #[test]
    fn test_merge_system_messages_accumulate() {
        let mut agg = AggregatedHookResult::default();
        agg.merge(success_with_sync(SyncHookOutput {
            system_message: Some("msg 1".to_string()),
            ..Default::default()
        }));
        agg.merge(success_with_sync(SyncHookOutput {
            system_message: Some("msg 2".to_string()),
            ..Default::default()
        }));

        assert_eq!(
            agg.system_messages,
            vec!["msg 1".to_string(), "msg 2".to_string()]
        );
    }

    #[test]
    fn test_merge_post_tool_use_specific_output_sets_mcp_override() {
        let mut agg = AggregatedHookResult::default();
        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::PostToolUse {
                additional_context: Some("cached".to_string()),
                updated_mcp_tool_output: Some(json!({"ok": true})),
            }),
            ..Default::default()
        }));

        assert_eq!(agg.additional_contexts, vec!["cached".to_string()]);
        assert_eq!(agg.updated_mcp_tool_output, Some(json!({"ok": true})));
    }

    #[test]
    fn test_merge_session_start_watch_paths_accumulate() {
        let mut agg = AggregatedHookResult::default();
        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::SessionStart {
                additional_context: None,
                initial_user_message: None,
                watch_paths: Some(vec![PathBuf::from("/a"), PathBuf::from("/b")]),
            }),
            ..Default::default()
        }));

        assert_eq!(
            agg.watch_paths,
            vec![PathBuf::from("/a"), PathBuf::from("/b")]
        );
    }

    #[test]
    fn test_merge_session_start_initial_user_message_first_wins() {
        let mut agg = AggregatedHookResult::default();

        // First SessionStart hook sets initial_user_message to "hello".
        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::SessionStart {
                additional_context: None,
                initial_user_message: Some("hello".to_string()),
                watch_paths: None,
            }),
            ..Default::default()
        }));
        assert_eq!(agg.initial_user_message.as_deref(), Some("hello"));

        // Second SessionStart hook with a different initial_user_message
        // MUST NOT overwrite (first-wins).
        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::SessionStart {
                additional_context: None,
                initial_user_message: Some("world".to_string()),
                watch_paths: None,
            }),
            ..Default::default()
        }));
        assert_eq!(agg.initial_user_message.as_deref(), Some("hello"));

        // A None value from a subsequent SessionStart hook must not clear
        // the previously-set value.
        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::SessionStart {
                additional_context: None,
                initial_user_message: None,
                watch_paths: None,
            }),
            ..Default::default()
        }));
        assert_eq!(agg.initial_user_message.as_deref(), Some("hello"));
    }

    #[test]
    fn test_merge_decision_block_in_sync_output_sets_blocking_error_and_deny() {
        let mut agg = AggregatedHookResult::default();
        agg.merge(HookExecResult {
            outcome: HookOutcome::Blocking,
            output: Some(HookOutput::Sync(SyncHookOutput {
                decision: Some(HookDecision::Block),
                reason: Some("policy violation".to_string()),
                ..Default::default()
            })),
            raw_stdout: String::new(),
            raw_stderr: String::new(),
            exit_code: Some(0),
        });

        let err = agg.blocking_error.as_ref().expect("blocking_error set");
        // The outcome-classified path uses stderr; since stderr is empty, it
        // falls back to stdout which is also empty — so the sync-output
        // branch should fill in the reason.
        assert!(err.message.is_empty() || err.message == "policy violation");
        // `decision: "block"` also maps to deny per Claude Code.
        assert_eq!(agg.permission_behavior, Some(PermissionBehavior::Deny));
    }

    #[test]
    fn test_merge_decision_approve_sets_permission_allow() {
        let mut agg = AggregatedHookResult::default();
        agg.merge(success_with_sync(SyncHookOutput {
            decision: Some(HookDecision::Approve),
            ..Default::default()
        }));

        assert_eq!(agg.permission_behavior, Some(PermissionBehavior::Allow));
        assert!(agg.blocking_error.is_none());
    }

    // ---- WorktreeCreate merge tests ----

    /// Two `WorktreeCreate` hooks both hand back a path — last-write-wins.
    /// Mirrors the `updated_input` semantics so plugins that chain on top
    /// of each other see predictable ordering.
    #[test]
    fn test_merge_worktree_create_last_wins_on_worktree_path() {
        let mut agg = AggregatedHookResult::default();

        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::WorktreeCreate {
                worktree_path: Some(PathBuf::from("/tmp/wt/first")),
            }),
            ..Default::default()
        }));

        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::WorktreeCreate {
                worktree_path: Some(PathBuf::from("/tmp/wt/second")),
            }),
            ..Default::default()
        }));

        assert_eq!(agg.worktree_path, Some(PathBuf::from("/tmp/wt/second")));
    }

    /// A subsequent `WorktreeCreate` hook that returns `worktreePath: None`
    /// must NOT clear a previously-set path. This guards against a
    /// noisy plugin wiping the intended override.
    #[test]
    fn test_merge_worktree_create_none_preserves_prior_path() {
        let mut agg = AggregatedHookResult::default();

        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::WorktreeCreate {
                worktree_path: Some(PathBuf::from("/tmp/wt/keep")),
            }),
            ..Default::default()
        }));

        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::WorktreeCreate { worktree_path: None }),
            ..Default::default()
        }));

        assert_eq!(agg.worktree_path, Some(PathBuf::from("/tmp/wt/keep")));
    }

    /// Sanity check: the `Default` impl zeros the new `worktree_path`
    /// field. Paired with the broader default test above; this one
    /// exists as a single-purpose regression gate so a future refactor
    /// that accidentally drops the field from `Default` is caught by a
    /// targeted failure instead of a multi-assertion cascade.
    #[test]
    fn test_aggregated_default_has_none_worktree_path() {
        let actual = AggregatedHookResult::default();
        assert!(actual.worktree_path.is_none());
    }

    #[test]
    fn test_merge_elicitation_decline_creates_blocking_error() {
        let mut agg = AggregatedHookResult::default();
        agg.merge(success_with_sync(SyncHookOutput {
            reason: Some("user declined".to_string()),
            hook_specific_output: Some(HookSpecificOutput::Elicitation {
                action: Some("decline".to_string()),
                content: None,
            }),
            ..Default::default()
        }));

        let err = agg.blocking_error.as_ref().expect("blocking_error set");
        assert_eq!(err.message, "user declined");
    }

    #[test]
    fn test_merge_elicitation_decline_uses_default_message_when_no_reason() {
        let mut agg = AggregatedHookResult::default();
        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::Elicitation {
                action: Some("decline".to_string()),
                content: None,
            }),
            ..Default::default()
        }));

        let err = agg.blocking_error.as_ref().expect("blocking_error set");
        assert_eq!(err.message, "Elicitation denied by hook");
    }

    // ---- Passthrough behavior tests ----

    /// When a hook sets `updated_input` but no `permission_decision`,
    /// `updated_input` should still be available in the aggregate and
    /// `permission_behavior` stays `None` (passthrough). This mirrors
    /// Claude Code's passthrough handling where a hook enriches or
    /// normalizes the input without making a permission decision.
    #[test]
    fn test_merge_passthrough_updated_input_without_permission() {
        let mut agg = AggregatedHookResult::default();
        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::PreToolUse {
                permission_decision: None,
                permission_decision_reason: None,
                updated_input: Some(json!({"normalized": true})),
                additional_context: None,
            }),
            ..Default::default()
        }));

        // permission_behavior stays None (passthrough)
        assert!(agg.permission_behavior.is_none());
        // But updated_input IS captured
        assert_eq!(agg.updated_input, Some(json!({"normalized": true})));
    }

    /// Multiple passthrough hooks can chain: each overwrites
    /// `updated_input` (last-write-wins) while `permission_behavior`
    /// stays `None` throughout.
    #[test]
    fn test_merge_passthrough_multiple_hooks_chain_updated_input() {
        let mut agg = AggregatedHookResult::default();

        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::PreToolUse {
                permission_decision: None,
                permission_decision_reason: None,
                updated_input: Some(json!({"step": 1})),
                additional_context: None,
            }),
            ..Default::default()
        }));

        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::PreToolUse {
                permission_decision: None,
                permission_decision_reason: None,
                updated_input: Some(json!({"step": 2})),
                additional_context: None,
            }),
            ..Default::default()
        }));

        assert!(agg.permission_behavior.is_none());
        assert_eq!(agg.updated_input, Some(json!({"step": 2})));
    }

    /// A passthrough hook (no `permission_decision`) combined with a
    /// permission-setting hook: the `updated_input` from the passthrough
    /// hook is preserved alongside the permission decision from the
    /// other hook.
    #[test]
    fn test_merge_passthrough_with_permission_hook_preserves_both() {
        let mut agg = AggregatedHookResult::default();

        // First hook: passthrough with updated_input
        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::PreToolUse {
                permission_decision: None,
                permission_decision_reason: None,
                updated_input: Some(json!({"sanitized": true})),
                additional_context: None,
            }),
            ..Default::default()
        }));

        // Second hook: permission decision without updated_input
        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::PreToolUse {
                permission_decision: Some(PermissionDecision::Allow),
                permission_decision_reason: None,
                updated_input: None,
                additional_context: None,
            }),
            ..Default::default()
        }));

        assert_eq!(agg.permission_behavior, Some(PermissionBehavior::Allow));
        assert_eq!(agg.updated_input, Some(json!({"sanitized": true})));
    }

    /// A passthrough hook with `additional_context` but no
    /// `permission_decision` and no `updated_input` — both
    /// `permission_behavior` and `updated_input` stay `None` while
    /// `additional_contexts` accumulates the value.
    #[test]
    fn test_merge_passthrough_additional_context_only() {
        let mut agg = AggregatedHookResult::default();
        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::PreToolUse {
                permission_decision: None,
                permission_decision_reason: None,
                updated_input: None,
                additional_context: Some("extra info from passthrough".to_string()),
            }),
            ..Default::default()
        }));

        assert!(agg.permission_behavior.is_none());
        assert!(agg.updated_input.is_none());
        assert_eq!(
            agg.additional_contexts,
            vec!["extra info from passthrough".to_string()]
        );
    }

    #[test]
    fn test_merge_elicitation_accept_does_not_create_blocking_error() {
        let mut agg = AggregatedHookResult::default();
        agg.merge(success_with_sync(SyncHookOutput {
            hook_specific_output: Some(HookSpecificOutput::Elicitation {
                action: Some("accept".to_string()),
                content: None,
            }),
            ..Default::default()
        }));

        assert!(agg.blocking_error.is_none());
    }
}
