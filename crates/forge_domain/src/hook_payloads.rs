//! In-process Rust payloads for the Claude-Code-style lifecycle events.
//!
//! These structs are the orchestrator-side shape of the new events added in
//! They travel inside [`crate::EventData`] and are handed to the
//! registered [`crate::EventHandle`] implementations when an event fires.
//!
//! They are distinct from [`crate::HookInputPayload`] (in `hook_io.rs`),
//! which is the *wire* shape written to a hook subprocess's stdin. The
//! `From` impls at the bottom of this file convert each in-process payload
//! into the matching wire variant so [`crate::PluginHookHandler`] can build
//! a [`crate::HookInput`] without each call site hand-rolling the mapping.
//!
//! Field naming mirrors Claude Code's schemas, so external hook binaries
//! (and test fixtures) see the same keys on both the in-process and wire
//! sides.
//!
//! References:
//! - Event schemas: `claude-code/src/entrypoints/sdk/coreSchemas.ts:387-796`
//! - Wire payload mirror: `crates/forge_domain/src/hook_io.rs:76-137`

use serde::{Deserialize, Serialize};

use crate::{HookInputPayload, PermissionBehavior};

// ---------- Tool lifecycle payloads ----------

/// Payload for the `PreToolUse` event — fired *before* a tool call runs.
///
/// Hooks can inspect the tool name and arguments, then either approve,
/// deny, or rewrite the input via
/// [`crate::HookSpecificOutput::PreToolUse`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreToolUsePayload {
    /// Name of the tool about to execute (e.g. `"Bash"`, `"Write"`).
    pub tool_name: String,
    /// Raw tool input as JSON — matches whatever the model emitted.
    pub tool_input: serde_json::Value,
    /// Unique ID correlating PreToolUse with the later PostToolUse or
    /// PostToolUseFailure for the same invocation.
    pub tool_use_id: String,
}

/// Payload for the `PostToolUse` event — fired after a tool call returns
/// successfully.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostToolUsePayload {
    /// Name of the tool that just executed.
    pub tool_name: String,
    /// Raw tool input as JSON.
    pub tool_input: serde_json::Value,
    /// Tool response as JSON (stdout / structured result / ...).
    pub tool_response: serde_json::Value,
    /// Same id as the paired [`PreToolUsePayload::tool_use_id`].
    pub tool_use_id: String,
}

/// Payload for the `PostToolUseFailure` event — fired after a tool call
/// errored (including user-interrupt cancellations).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostToolUseFailurePayload {
    /// Name of the tool that failed.
    pub tool_name: String,
    /// Raw tool input as JSON — preserved for diagnostic hooks.
    pub tool_input: serde_json::Value,
    /// Same id as the paired [`PreToolUsePayload::tool_use_id`].
    pub tool_use_id: String,
    /// Error message as surfaced to the model.
    pub error: String,
    /// `Some(true)` when the failure was caused by a user interrupt
    /// rather than a tool-side error. Optional so most tool failures can
    /// leave it unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_interrupt: Option<bool>,
}

// ---------- User-facing events ----------

/// Payload for the `UserPromptSubmit` event — fired when the user submits
/// a new prompt to the agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserPromptSubmitPayload {
    /// Raw prompt text as entered by the user, before any transformation.
    pub prompt: String,
}

// ---------- Session events ----------

/// Why a new session is starting. Serialized as lowercase strings
/// (`"startup"`, `"resume"`, ...) to match Claude Code's wire format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStartSource {
    /// Fresh session — the process just booted up.
    Startup,
    /// An existing session was loaded from disk.
    Resume,
    /// The user explicitly cleared the conversation.
    Clear,
    /// A compaction cycle finished and the session is being reset with a
    /// summary.
    Compact,
}

impl SessionStartSource {
    /// Lowercase wire string (`"startup"`, `"resume"`, ...).
    pub fn as_wire_str(self) -> &'static str {
        match self {
            SessionStartSource::Startup => "startup",
            SessionStartSource::Resume => "resume",
            SessionStartSource::Clear => "clear",
            SessionStartSource::Compact => "compact",
        }
    }
}

/// Payload for the `SessionStart` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionStartPayload {
    /// How the session started.
    pub source: SessionStartSource,
    /// Optional model identifier (plain string so the wire shape is
    /// schema-compatible with Claude Code, which also emits a bare
    /// string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Why a session ended. Serialized as snake_case strings
/// (`"clear"`, `"prompt_input_exit"`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionEndReason {
    /// User cleared the conversation.
    Clear,
    /// Session was resumed into another one.
    Resume,
    /// User logged out.
    Logout,
    /// Prompt input loop exited (e.g. REPL closed).
    PromptInputExit,
    /// Bypass-permissions mode was disabled, ending the session.
    BypassPermissionsDisabled,
    /// Fallback for any other reason.
    Other,
}

impl SessionEndReason {
    /// snake_case wire string (`"clear"`, `"prompt_input_exit"`, ...).
    pub fn as_wire_str(self) -> &'static str {
        match self {
            SessionEndReason::Clear => "clear",
            SessionEndReason::Resume => "resume",
            SessionEndReason::Logout => "logout",
            SessionEndReason::PromptInputExit => "prompt_input_exit",
            SessionEndReason::BypassPermissionsDisabled => "bypass_permissions_disabled",
            SessionEndReason::Other => "other",
        }
    }
}

/// Payload for the `SessionEnd` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionEndPayload {
    pub reason: SessionEndReason,
}

// ---------- Stop events ----------

/// Payload for the `Stop` event — fired when the agent loop finishes a
/// turn naturally (e.g. model produced an assistant message with no tool
/// calls).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopPayload {
    /// `true` iff a stop hook is currently running — a guard used by
    /// Claude Code to prevent stop-hook recursion.
    pub stop_hook_active: bool,
    /// Optional last assistant message body (plain text) to give the hook
    /// some context when deciding whether to block the stop.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_assistant_message: Option<String>,
}

/// Payload for the `StopFailure` event — fired when the agent loop halts
/// due to an error (as opposed to a clean stop).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopFailurePayload {
    /// The error message that caused the halt.
    pub error: String,
    /// Optional additional details about the error (e.g. HTTP status text).
    /// Mirrors Claude Code's `error_details: z.string().optional()` field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_details: Option<String>,
    /// Optional last assistant message body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_assistant_message: Option<String>,
}

// ---------- Notification events ----------

/// Kind of user-facing notification. Serialized as snake_case on the wire
/// (`"idle_prompt"`, `"auth_success"`, ...).
///
/// The set is intentionally closed for now — only the four notification
/// sources below are supported (REPL idle, OAuth completion, elicitation).
/// A free-form `Custom(String)` variant can be added later without
/// breaking the wire format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationKind {
    /// REPL has been idle waiting for user input past the configured
    /// threshold.
    IdlePrompt,
    /// OAuth / credential flow completed successfully.
    AuthSuccess,
    /// An elicitation (interactive prompt) just finished.
    ElicitationComplete,
    /// The user provided a response to an elicitation.
    ElicitationResponse,
}

impl NotificationKind {
    /// Lowercase snake_case wire string (`"idle_prompt"`,
    /// `"auth_success"`, ...).
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::IdlePrompt => "idle_prompt",
            Self::AuthSuccess => "auth_success",
            Self::ElicitationComplete => "elicitation_complete",
            Self::ElicitationResponse => "elicitation_response",
        }
    }
}

/// Payload for the `Notification` event — fired when Forge wants to
/// surface a user-facing notification (idle prompt, OAuth success, ...).
///
/// The `notification_type` field holds the already-serialized
/// [`NotificationKind`] so the same struct doubles as the in-process
/// payload and as the input to the `From<NotificationPayload> for
/// HookInputPayload` impl below. This event is not yet fired — real
/// emission points are pending.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationPayload {
    /// Body of the notification as shown to the user.
    pub message: String,
    /// Optional short title (e.g. `"Authentication complete"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Serialized [`NotificationKind`] wire string
    /// (`"idle_prompt"`, ...). Stored as a plain `String` so hook matchers
    /// can filter on it without needing to know the enum variants.
    pub notification_type: String,
}

// ---------- Setup events ----------

/// Why Forge is running a setup pass. Serialized as snake_case on the
/// wire (`"init"`, `"maintenance"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupTrigger {
    /// First-time setup (`forge --init` or equivalent).
    Init,
    /// Periodic maintenance sweep (`forge --maintenance` or equivalent).
    Maintenance,
}

impl SetupTrigger {
    /// Lowercase snake_case wire string (`"init"` / `"maintenance"`).
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::Init => "init",
            Self::Maintenance => "maintenance",
        }
    }
}

/// Payload for the `Setup` event — fired once per `forge --init` /
/// `forge --maintenance` invocation.
///
/// Currently ships only the infrastructure (payload + dispatcher impl).
/// The CLI flags and fire site in `forge_main` are pending.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupPayload {
    /// What triggered the setup pass.
    pub trigger: SetupTrigger,
}

// ---------- Config change events ----------

/// Which configuration store emitted a change. Serialized as snake_case
/// strings on the wire (`"user_settings"`, `"project_settings"`, ...)
/// so plugin hooks can filter on the `source` matcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigSource {
    /// User-level settings (e.g. `~/forge/.forge.toml`).
    UserSettings,
    /// Project-level settings checked into the repo
    /// (e.g. `<project>/.forge/config.toml`).
    ProjectSettings,
    /// Local machine overrides (e.g. `<project>/.forge/local.toml`).
    LocalSettings,
    /// Policy file installed by an administrator.
    PolicySettings,
    /// Skills directory — any add/remove/update of a skill file.
    Skills,
    /// Hooks configuration (`hooks.json` under any config root).
    Hooks,
    /// Installed plugins directory — any add/remove/update of a plugin.
    Plugins,
}

impl ConfigSource {
    /// snake_case wire string matching the `#[serde]` representation.
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::UserSettings => "user_settings",
            Self::ProjectSettings => "project_settings",
            Self::LocalSettings => "local_settings",
            Self::PolicySettings => "policy_settings",
            Self::Skills => "skills",
            Self::Hooks => "hooks",
            Self::Plugins => "plugins",
        }
    }
}

/// Payload for the `ConfigChange` event — fired when the
/// [`crate::HookInputPayload::ConfigChange`] wire event is raised by
/// the `ConfigWatcher` service after a debounced filesystem change.
///
/// The `file_path` is optional because some config sources are
/// directory-level (e.g. `Plugins`, `Skills`) and callers may pass the
/// directory root rather than a specific file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigChangePayload {
    /// Which config store the change came from.
    pub source: ConfigSource,
    /// Optional absolute path of the file (or directory) that changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<std::path::PathBuf>,
}

// ---------- Compaction events ----------

/// What triggered a compaction cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompactTrigger {
    /// User ran the `/compact` command explicitly.
    Manual,
    /// Forge auto-compacted because context usage crossed a threshold.
    Auto,
}

impl CompactTrigger {
    /// Lowercase wire string (`"manual"` / `"auto"`).
    pub fn as_wire_str(self) -> &'static str {
        match self {
            CompactTrigger::Manual => "manual",
            CompactTrigger::Auto => "auto",
        }
    }
}

/// Payload for the `PreCompact` event — fired just before a compaction
/// cycle starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreCompactPayload {
    /// How the compaction was triggered.
    pub trigger: CompactTrigger,
    /// Optional free-form instructions from the user (only present for
    /// manual `/compact <text>` invocations).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_instructions: Option<String>,
}

/// Payload for the `PostCompact` event — fired after compaction finishes
/// and the new summary is available.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostCompactPayload {
    /// How the compaction was triggered (mirrors the earlier PreCompact
    /// payload for symmetry).
    pub trigger: CompactTrigger,
    /// The summary text produced by the compaction pass.
    pub compact_summary: String,
}

// ---------- Subagent events ----------

/// Payload for the `SubagentStart` event — fired when a sub-agent begins
/// running inside the orchestrator (e.g. a spawned `code-reviewer` or
/// `muse` agent).
///
/// Currently ships only the infrastructure slot; the real fire sites in
/// `agent_executor.rs` are pending.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentStartPayload {
    /// Stable identifier for the running sub-agent instance. Used by
    /// plugin hooks to correlate a `SubagentStart` with its paired
    /// `SubagentStop`.
    pub agent_id: String,
    /// The sub-agent type (e.g. `"forge"`, `"code-reviewer"`). Matchers
    /// filter on this field.
    pub agent_type: String,
}

/// Payload for the `SubagentStop` event — fired when a sub-agent
/// finishes its turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentStopPayload {
    /// Stable identifier for the sub-agent instance — mirrors the paired
    /// [`SubagentStartPayload::agent_id`].
    pub agent_id: String,
    /// The sub-agent type (matchers filter on this field).
    pub agent_type: String,
    /// Absolute path to the sub-agent's own transcript file, distinct
    /// from the parent session's transcript.
    pub agent_transcript_path: std::path::PathBuf,
    /// `true` iff a stop hook is already running — guard used to prevent
    /// stop-hook recursion, mirroring [`StopPayload::stop_hook_active`].
    pub stop_hook_active: bool,
    /// Optional last assistant message body emitted by the sub-agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_assistant_message: Option<String>,
}

// ---------- Permission events ----------

/// Where a permission rule update should be persisted. Serialized as
/// camelCase strings on the wire (`"userSettings"`, `"projectSettings"`,
/// ...) to match Claude Code's permission-rule schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionDestination {
    /// Persist into the user-level settings file.
    UserSettings,
    /// Persist into the project-level settings file.
    ProjectSettings,
    /// Persist into the local machine-only overrides file.
    LocalSettings,
    /// Apply only to the current session without persisting.
    Session,
}

/// A single permission rule update suggestion emitted alongside a
/// [`PermissionRequestPayload`] so plugin hooks can propose adding the
/// requested tool to one of the permission stores.
///
/// Mirrors Claude Code's `permissionUpdates` schema field on the
/// PermissionRequest event. Currently ships only the type — computing
/// actual suggestions (and wiring them through the policy engine)
/// is pending.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionUpdate {
    /// List of permission rule strings to add (e.g. `"Bash(git *)"`).
    pub rules: Vec<String>,
    /// How the rules should take effect (allow / deny / ask).
    pub behavior: PermissionBehavior,
    /// Where the rules should be persisted.
    pub destination: PermissionDestination,
}

/// Payload for the `PermissionRequest` event — fired when a tool call
/// needs permission that hasn't been granted yet and the policy engine
/// wants to let plugin hooks suggest or auto-allow it.
///
/// Currently ships only the payload + dispatcher infra; fire sites in
/// `policy.rs` are pending.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionRequestPayload {
    /// Name of the tool requesting permission.
    pub tool_name: String,
    /// Raw tool input as JSON — matches whatever the model emitted.
    pub tool_input: serde_json::Value,
    /// Suggested permission updates computed by the policy engine.
    /// Currently empty — populated once the real suggestion logic lands.
    pub permission_suggestions: Vec<PermissionUpdate>,
}

/// Payload for the `PermissionDenied` event — fired after a permission
/// request is rejected (either by a plugin hook or by the user).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionDeniedPayload {
    /// Name of the tool whose invocation was denied.
    pub tool_name: String,
    /// Raw tool input as JSON.
    pub tool_input: serde_json::Value,
    /// Tool use id correlating the denied call with earlier
    /// `PermissionRequest` / `PreToolUse` events.
    pub tool_use_id: String,
    /// Human-readable reason surfaced alongside the denial.
    pub reason: String,
}

// ---------- Cwd + FileChanged events ----------

/// Payload for the `CwdChanged` event — fired whenever the
/// orchestrator's current working directory changes (e.g. after
/// `cd` inside a shell tool, or when switching worktrees).
///
/// Currently ships only the payload + dispatcher infra; cwd tracking
/// inside the `Shell` tool is pending.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CwdChangedPayload {
    /// The working directory before the change.
    pub old_cwd: std::path::PathBuf,
    /// The working directory after the change.
    pub new_cwd: std::path::PathBuf,
}

/// Kind of filesystem event reported by a file watcher. Serialized as
/// snake_case strings on the wire (`"change"`, `"add"`, `"unlink"`) to
/// match Claude Code's `FileChanged` event schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileChangeEvent {
    /// File contents changed.
    Change,
    /// File was created.
    Add,
    /// File was removed.
    Unlink,
}

impl FileChangeEvent {
    /// snake_case wire string (`"change"`, `"add"`, `"unlink"`).
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::Change => "change",
            Self::Add => "add",
            Self::Unlink => "unlink",
        }
    }
}

/// Payload for the `FileChanged` event — fired when a watched path on
/// disk changes.
///
/// Currently ships only the payload + dispatcher infra; the real
/// `FileChangedWatcher` service that fires this event is pending.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChangedPayload {
    /// Absolute path of the file that changed.
    pub file_path: std::path::PathBuf,
    /// What kind of change occurred.
    pub event: FileChangeEvent,
}

// ---------- Worktree events ----------

/// Payload for the `WorktreeCreate` event — fired when the agent enters
/// a new git worktree via `EnterWorktreeTool` or when a hook-driven VCS
/// integration provisions one on its behalf.
///
/// Currently ships only the payload + dispatcher plumbing; the real fire
/// sites in the worktree tools and sandbox layer are pending.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeCreatePayload {
    /// User-provided name for the worktree. Plugin hooks receive this as
    /// the matcher field so they can namespace worktree creation logic
    /// (e.g., per-project VCS adapters).
    pub name: String,
}

/// Payload for the `WorktreeRemove` event — fired when the agent leaves
/// a worktree via `ExitWorktreeTool`, either through git or via a
/// plugin-provided VCS hook.
///
/// Currently ships only the payload; real fire sites are pending.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeRemovePayload {
    /// Absolute path of the worktree that was removed.
    pub worktree_path: std::path::PathBuf,
}

// ---------- InstructionsLoaded event ----------
//
// The [`MemoryType`] and [`InstructionsLoadReason`] enums referenced by
// the payload below used to live inline in this file. They were
// moved into `crate::memory` so the in-process
// [`crate::LoadedInstructions`] struct can share the same classification
// vocabulary without a circular dependency. They are re-exported at the
// crate root so the payload continues to reference them via plain
// `MemoryType` / `InstructionsLoadReason` below.

use crate::{InstructionsLoadReason, MemoryType};

/// Payload for the `InstructionsLoaded` event — fired whenever
/// Forge loads an instructions / memory file (`AGENTS.md` etc).
///
/// Currently ships only the payload + dispatcher plumbing; the
/// full multi-layer memory system with nested traversal, conditional
/// rules, and `@include` resolution is pending. The existing
/// `CustomInstructionsService` is **not** modified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstructionsLoadedPayload {
    pub file_path: std::path::PathBuf,
    pub memory_type: MemoryType,
    pub load_reason: InstructionsLoadReason,
    /// Optional conditional-rule globs from frontmatter `paths:`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub globs: Option<Vec<String>>,
    /// Path of the file whose access triggered this load (nested
    /// traversal case). Always None for `SessionStart` loads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_file_path: Option<std::path::PathBuf>,
    /// Path of the parent instructions file when loaded via `@include`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_file_path: Option<std::path::PathBuf>,
}

// ---------- Elicitation events ----------

/// Payload for the `Elicitation` event — fired by the MCP client
/// before it prompts the user for additional input on behalf of an
/// MCP server.
///
/// Currently ships only the payload + dispatcher plumbing; the
/// actual MCP client integration (handling `elicitation/create`
/// requests from servers, terminal UI for form/URL modes) is pending.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElicitationPayload {
    /// Name of the MCP server that requested the elicitation.
    pub server_name: String,
    /// Human-readable prompt message shown to the user.
    pub message: String,
    /// JSON Schema describing the requested form fields. Populated in
    /// form mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_schema: Option<serde_json::Value>,
    /// Elicitation mode — `"form"` or `"url"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// URL to open in the user's browser. Populated in url mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Unique identifier for the elicitation, used to correlate
    /// `Elicitation` with `ElicitationResult`. Mirrors Claude Code's
    /// `elicitation_id: z.string().optional()` field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elicitation_id: Option<String>,
}

/// Payload for the `ElicitationResult` event — fired after the user
/// (or an auto-responding plugin hook) completes the elicitation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElicitationResultPayload {
    pub server_name: String,
    /// One of `"accept"`, `"decline"`, `"cancel"`.
    pub action: String,
    /// User-provided form data (form mode only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<serde_json::Value>,
    /// Elicitation mode — `"form"` or `"url"`. Mirrors Claude Code's
    /// `mode: z.enum(['form', 'url']).optional()` on ElicitationResult.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// Mirrors Claude Code's `elicitation_id: z.string().optional()`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elicitation_id: Option<String>,
}

// ---------- Conversions to wire payloads ----------
//
// Each `From<...> for HookInputPayload` impl pairs the in-process payload
// with its wire shape in `hook_io.rs`. When a new payload ships without a
// matching wire variant, fall back to `HookInputPayload::Generic`.

impl From<PreToolUsePayload> for HookInputPayload {
    fn from(p: PreToolUsePayload) -> Self {
        HookInputPayload::PreToolUse {
            tool_name: p.tool_name,
            tool_input: p.tool_input,
            tool_use_id: p.tool_use_id,
        }
    }
}

impl From<PostToolUsePayload> for HookInputPayload {
    fn from(p: PostToolUsePayload) -> Self {
        HookInputPayload::PostToolUse {
            tool_name: p.tool_name,
            tool_input: p.tool_input,
            tool_response: p.tool_response,
            tool_use_id: p.tool_use_id,
        }
    }
}

impl From<PostToolUseFailurePayload> for HookInputPayload {
    fn from(p: PostToolUseFailurePayload) -> Self {
        HookInputPayload::PostToolUseFailure {
            tool_name: p.tool_name,
            tool_input: p.tool_input,
            tool_use_id: p.tool_use_id,
            error: p.error,
            is_interrupt: p.is_interrupt,
        }
    }
}

impl From<UserPromptSubmitPayload> for HookInputPayload {
    fn from(p: UserPromptSubmitPayload) -> Self {
        HookInputPayload::UserPromptSubmit { prompt: p.prompt }
    }
}

impl From<SessionStartPayload> for HookInputPayload {
    fn from(p: SessionStartPayload) -> Self {
        HookInputPayload::SessionStart {
            source: p.source.as_wire_str().to_string(),
            model: p.model,
        }
    }
}

impl From<SessionEndPayload> for HookInputPayload {
    fn from(p: SessionEndPayload) -> Self {
        HookInputPayload::SessionEnd { reason: p.reason.as_wire_str().to_string() }
    }
}

impl From<StopPayload> for HookInputPayload {
    fn from(p: StopPayload) -> Self {
        HookInputPayload::Stop {
            stop_hook_active: p.stop_hook_active,
            last_assistant_message: p.last_assistant_message,
        }
    }
}

impl From<StopFailurePayload> for HookInputPayload {
    fn from(p: StopFailurePayload) -> Self {
        HookInputPayload::StopFailure {
            error: p.error,
            error_details: p.error_details,
            last_assistant_message: p.last_assistant_message,
        }
    }
}

impl From<PreCompactPayload> for HookInputPayload {
    fn from(p: PreCompactPayload) -> Self {
        HookInputPayload::PreCompact {
            trigger: p.trigger.as_wire_str().to_string(),
            custom_instructions: p.custom_instructions,
        }
    }
}

impl From<PostCompactPayload> for HookInputPayload {
    fn from(p: PostCompactPayload) -> Self {
        HookInputPayload::PostCompact {
            trigger: p.trigger.as_wire_str().to_string(),
            compact_summary: p.compact_summary,
        }
    }
}

impl From<NotificationPayload> for HookInputPayload {
    fn from(p: NotificationPayload) -> Self {
        HookInputPayload::Notification {
            message: p.message,
            title: p.title,
            notification_type: p.notification_type,
        }
    }
}

impl From<SetupPayload> for HookInputPayload {
    fn from(p: SetupPayload) -> Self {
        HookInputPayload::Setup { trigger: p.trigger.as_wire_str().to_string() }
    }
}

impl From<ConfigChangePayload> for HookInputPayload {
    fn from(p: ConfigChangePayload) -> Self {
        HookInputPayload::ConfigChange {
            source: p.source.as_wire_str().to_string(),
            file_path: p.file_path,
        }
    }
}

impl From<SubagentStartPayload> for HookInputPayload {
    fn from(p: SubagentStartPayload) -> Self {
        HookInputPayload::SubagentStart { agent_id: p.agent_id, agent_type: p.agent_type }
    }
}

impl From<SubagentStopPayload> for HookInputPayload {
    fn from(p: SubagentStopPayload) -> Self {
        HookInputPayload::SubagentStop {
            agent_id: p.agent_id,
            agent_type: p.agent_type,
            agent_transcript_path: p.agent_transcript_path,
            stop_hook_active: p.stop_hook_active,
            last_assistant_message: p.last_assistant_message,
        }
    }
}

impl From<PermissionRequestPayload> for HookInputPayload {
    fn from(p: PermissionRequestPayload) -> Self {
        HookInputPayload::PermissionRequest {
            tool_name: p.tool_name,
            tool_input: p.tool_input,
            permission_suggestions: p.permission_suggestions,
        }
    }
}

impl From<PermissionDeniedPayload> for HookInputPayload {
    fn from(p: PermissionDeniedPayload) -> Self {
        HookInputPayload::PermissionDenied {
            tool_name: p.tool_name,
            tool_input: p.tool_input,
            tool_use_id: p.tool_use_id,
            reason: p.reason,
        }
    }
}

impl From<CwdChangedPayload> for HookInputPayload {
    fn from(p: CwdChangedPayload) -> Self {
        HookInputPayload::CwdChanged { old_cwd: p.old_cwd, new_cwd: p.new_cwd }
    }
}

impl From<FileChangedPayload> for HookInputPayload {
    fn from(p: FileChangedPayload) -> Self {
        HookInputPayload::FileChanged {
            file_path: p.file_path,
            event: p.event.as_wire_str().to_string(),
        }
    }
}

impl From<WorktreeCreatePayload> for HookInputPayload {
    fn from(p: WorktreeCreatePayload) -> Self {
        HookInputPayload::WorktreeCreate { name: p.name }
    }
}

impl From<WorktreeRemovePayload> for HookInputPayload {
    fn from(p: WorktreeRemovePayload) -> Self {
        HookInputPayload::WorktreeRemove { worktree_path: p.worktree_path }
    }
}

impl From<InstructionsLoadedPayload> for HookInputPayload {
    fn from(p: InstructionsLoadedPayload) -> Self {
        HookInputPayload::InstructionsLoaded {
            file_path: p.file_path,
            memory_type: p.memory_type.as_wire_str().to_string(),
            load_reason: p.load_reason.as_wire_str().to_string(),
            globs: p.globs,
            trigger_file_path: p.trigger_file_path,
            parent_file_path: p.parent_file_path,
        }
    }
}

impl From<ElicitationPayload> for HookInputPayload {
    fn from(p: ElicitationPayload) -> Self {
        HookInputPayload::Elicitation {
            server_name: p.server_name,
            message: p.message,
            requested_schema: p.requested_schema,
            mode: p.mode,
            url: p.url,
            elicitation_id: p.elicitation_id,
        }
    }
}

impl From<ElicitationResultPayload> for HookInputPayload {
    fn from(p: ElicitationResultPayload) -> Self {
        HookInputPayload::ElicitationResult {
            server_name: p.server_name,
            action: p.action,
            content: p.content,
            mode: p.mode,
            elicitation_id: p.elicitation_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_session_start_source_serializes_as_lowercase() {
        assert_eq!(
            serde_json::to_string(&SessionStartSource::Startup).unwrap(),
            "\"startup\""
        );
        assert_eq!(
            serde_json::to_string(&SessionStartSource::Compact).unwrap(),
            "\"compact\""
        );
    }

    #[test]
    fn test_session_end_reason_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&SessionEndReason::PromptInputExit).unwrap(),
            "\"prompt_input_exit\""
        );
        assert_eq!(
            serde_json::to_string(&SessionEndReason::BypassPermissionsDisabled).unwrap(),
            "\"bypass_permissions_disabled\""
        );
    }

    #[test]
    fn test_compact_trigger_serializes_as_lowercase() {
        assert_eq!(
            serde_json::to_string(&CompactTrigger::Manual).unwrap(),
            "\"manual\""
        );
        assert_eq!(
            serde_json::to_string(&CompactTrigger::Auto).unwrap(),
            "\"auto\""
        );
    }

    #[test]
    fn test_pre_tool_use_payload_serializes_with_snake_case_fields() {
        let fixture = PreToolUsePayload {
            tool_name: "Bash".to_string(),
            tool_input: json!({"command": "ls"}),
            tool_use_id: "toolu_01".to_string(),
        };
        let actual = serde_json::to_value(&fixture).unwrap();
        assert_eq!(actual["tool_name"], "Bash");
        assert_eq!(actual["tool_input"]["command"], "ls");
        assert_eq!(actual["tool_use_id"], "toolu_01");
    }

    #[test]
    fn test_post_tool_use_failure_omits_is_interrupt_when_none() {
        let fixture = PostToolUseFailurePayload {
            tool_name: "Bash".to_string(),
            tool_input: json!({}),
            tool_use_id: "t1".to_string(),
            error: "boom".to_string(),
            is_interrupt: None,
        };
        let actual = serde_json::to_value(&fixture).unwrap();
        assert!(actual.get("is_interrupt").is_none());
        assert_eq!(actual["error"], "boom");
    }

    #[test]
    fn test_pre_tool_use_payload_into_hook_input_payload() {
        let fixture = PreToolUsePayload {
            tool_name: "Write".to_string(),
            tool_input: json!({"path": "/tmp/x"}),
            tool_use_id: "t42".to_string(),
        };
        let actual: HookInputPayload = fixture.into();
        match actual {
            HookInputPayload::PreToolUse { tool_name, tool_use_id, .. } => {
                assert_eq!(tool_name, "Write");
                assert_eq!(tool_use_id, "t42");
            }
            other => panic!("expected PreToolUse wire variant, got {other:?}"),
        }
    }

    #[test]
    fn test_session_start_payload_into_hook_input_payload_maps_source_string() {
        let fixture = SessionStartPayload {
            source: SessionStartSource::Resume,
            model: Some("m".to_string()),
        };
        let actual: HookInputPayload = fixture.into();
        match actual {
            HookInputPayload::SessionStart { source, model } => {
                assert_eq!(source, "resume");
                assert_eq!(model.as_deref(), Some("m"));
            }
            other => panic!("expected SessionStart wire variant, got {other:?}"),
        }
    }

    #[test]
    fn test_session_end_payload_into_hook_input_payload_maps_reason_string() {
        let fixture = SessionEndPayload { reason: SessionEndReason::PromptInputExit };
        let actual: HookInputPayload = fixture.into();
        match actual {
            HookInputPayload::SessionEnd { reason } => {
                assert_eq!(reason, "prompt_input_exit");
            }
            other => panic!("expected SessionEnd wire variant, got {other:?}"),
        }
    }

    #[test]
    fn test_pre_compact_payload_into_hook_input_payload_maps_trigger_string() {
        let fixture = PreCompactPayload {
            trigger: CompactTrigger::Auto,
            custom_instructions: Some("short".to_string()),
        };
        let actual: HookInputPayload = fixture.into();
        match actual {
            HookInputPayload::PreCompact { trigger, custom_instructions } => {
                assert_eq!(trigger, "auto");
                assert_eq!(custom_instructions.as_deref(), Some("short"));
            }
            other => panic!("expected PreCompact wire variant, got {other:?}"),
        }
    }

    #[test]
    fn test_post_compact_payload_into_hook_input_payload() {
        let fixture = PostCompactPayload {
            trigger: CompactTrigger::Manual,
            compact_summary: "all good".to_string(),
        };
        let actual: HookInputPayload = fixture.into();
        match actual {
            HookInputPayload::PostCompact { trigger, compact_summary } => {
                assert_eq!(trigger, "manual");
                assert_eq!(compact_summary, "all good");
            }
            other => panic!("expected PostCompact wire variant, got {other:?}"),
        }
    }

    #[test]
    fn test_stop_payload_into_hook_input_payload() {
        let fixture = StopPayload {
            stop_hook_active: true,
            last_assistant_message: Some("done".to_string()),
        };
        let actual: HookInputPayload = fixture.into();
        match actual {
            HookInputPayload::Stop { stop_hook_active, last_assistant_message } => {
                assert!(stop_hook_active);
                assert_eq!(last_assistant_message.as_deref(), Some("done"));
            }
            other => panic!("expected Stop wire variant, got {other:?}"),
        }
    }

    // ---- Notification payload tests ----

    #[test]
    fn test_notification_kind_as_wire_str_covers_all_variants() {
        assert_eq!(NotificationKind::IdlePrompt.as_wire_str(), "idle_prompt");
        assert_eq!(NotificationKind::AuthSuccess.as_wire_str(), "auth_success");
        assert_eq!(
            NotificationKind::ElicitationComplete.as_wire_str(),
            "elicitation_complete"
        );
        assert_eq!(
            NotificationKind::ElicitationResponse.as_wire_str(),
            "elicitation_response"
        );
    }

    #[test]
    fn test_notification_payload_serializes_with_snake_case_fields() {
        let fixture = NotificationPayload {
            message: "OAuth complete".to_string(),
            title: Some("Authenticated".to_string()),
            notification_type: NotificationKind::AuthSuccess.as_wire_str().to_string(),
        };
        let actual = serde_json::to_value(&fixture).unwrap();
        assert_eq!(actual["message"], "OAuth complete");
        assert_eq!(actual["title"], "Authenticated");
        assert_eq!(actual["notification_type"], "auth_success");
    }

    #[test]
    fn test_notification_payload_omits_title_when_none() {
        let fixture = NotificationPayload {
            message: "idle".to_string(),
            title: None,
            notification_type: NotificationKind::IdlePrompt.as_wire_str().to_string(),
        };
        let actual = serde_json::to_value(&fixture).unwrap();
        assert!(actual.get("title").is_none());
        assert_eq!(actual["notification_type"], "idle_prompt");
    }

    #[test]
    fn test_notification_payload_into_hook_input_payload() {
        let fixture = NotificationPayload {
            message: "idle for a while".to_string(),
            title: None,
            notification_type: NotificationKind::IdlePrompt.as_wire_str().to_string(),
        };
        let actual: HookInputPayload = fixture.into();
        match actual {
            HookInputPayload::Notification { message, title, notification_type } => {
                assert_eq!(message, "idle for a while");
                assert_eq!(title, None);
                assert_eq!(notification_type, "idle_prompt");
            }
            other => panic!("expected Notification wire variant, got {other:?}"),
        }
    }

    // ---- ConfigChange payload tests ----

    #[test]
    fn test_config_source_wire_str_all_variants() {
        assert_eq!(ConfigSource::UserSettings.as_wire_str(), "user_settings");
        assert_eq!(
            ConfigSource::ProjectSettings.as_wire_str(),
            "project_settings"
        );
        assert_eq!(ConfigSource::LocalSettings.as_wire_str(), "local_settings");
        assert_eq!(
            ConfigSource::PolicySettings.as_wire_str(),
            "policy_settings"
        );
        assert_eq!(ConfigSource::Skills.as_wire_str(), "skills");
        assert_eq!(ConfigSource::Hooks.as_wire_str(), "hooks");
        assert_eq!(ConfigSource::Plugins.as_wire_str(), "plugins");
    }

    #[test]
    fn test_config_change_payload_serialization() {
        // snake_case enum tag for the source and omitted file_path when None.
        let fixture =
            ConfigChangePayload { source: ConfigSource::ProjectSettings, file_path: None };
        let actual = serde_json::to_value(&fixture).unwrap();
        assert_eq!(actual["source"], "project_settings");
        assert!(actual.get("file_path").is_none());

        // With file_path populated the field is serialized as-is
        // (snake_case, plain string path).
        let fixture = ConfigChangePayload {
            source: ConfigSource::UserSettings,
            file_path: Some(std::path::PathBuf::from("/home/u/.forge/config.toml")),
        };
        let actual = serde_json::to_value(&fixture).unwrap();
        assert_eq!(actual["source"], "user_settings");
        assert_eq!(actual["file_path"], "/home/u/.forge/config.toml");
    }

    #[test]
    fn test_config_change_payload_into_hook_input_payload() {
        let fixture = ConfigChangePayload {
            source: ConfigSource::Plugins,
            file_path: Some(std::path::PathBuf::from("/plugins/x")),
        };
        let actual: HookInputPayload = fixture.into();
        match actual {
            HookInputPayload::ConfigChange { source, file_path } => {
                assert_eq!(source, "plugins");
                assert_eq!(file_path, Some(std::path::PathBuf::from("/plugins/x")));
            }
            other => panic!("expected ConfigChange wire variant, got {other:?}"),
        }
    }

    // ---- Setup payload tests ----

    #[test]
    fn test_setup_trigger_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&SetupTrigger::Init).unwrap(),
            "\"init\""
        );
        assert_eq!(
            serde_json::to_string(&SetupTrigger::Maintenance).unwrap(),
            "\"maintenance\""
        );
    }

    #[test]
    fn test_setup_payload_into_hook_input_payload_maps_trigger_string() {
        let fixture = SetupPayload { trigger: SetupTrigger::Maintenance };
        let actual: HookInputPayload = fixture.into();
        match actual {
            HookInputPayload::Setup { trigger } => {
                assert_eq!(trigger, "maintenance");
            }
            other => panic!("expected Setup wire variant, got {other:?}"),
        }
    }

    // ---- Subagent payload tests ----

    #[test]
    fn test_subagent_start_payload_serializes_with_snake_case_fields() {
        let fixture = SubagentStartPayload {
            agent_id: "agent-xyz".to_string(),
            agent_type: "code-reviewer".to_string(),
        };
        let actual = serde_json::to_value(&fixture).unwrap();
        assert_eq!(actual["agent_id"], "agent-xyz");
        assert_eq!(actual["agent_type"], "code-reviewer");
    }

    #[test]
    fn test_subagent_start_payload_into_hook_input_payload() {
        let fixture = SubagentStartPayload {
            agent_id: "agent-1".to_string(),
            agent_type: "muse".to_string(),
        };
        let actual: HookInputPayload = fixture.into();
        match actual {
            HookInputPayload::SubagentStart { agent_id, agent_type } => {
                assert_eq!(agent_id, "agent-1");
                assert_eq!(agent_type, "muse");
            }
            other => panic!("expected SubagentStart wire variant, got {other:?}"),
        }
    }

    #[test]
    fn test_subagent_stop_payload_serializes_omits_last_message_when_none() {
        let fixture = SubagentStopPayload {
            agent_id: "agent-1".to_string(),
            agent_type: "forge".to_string(),
            agent_transcript_path: std::path::PathBuf::from("/tmp/sub.jsonl"),
            stop_hook_active: false,
            last_assistant_message: None,
        };
        let actual = serde_json::to_value(&fixture).unwrap();
        assert_eq!(actual["agent_id"], "agent-1");
        assert_eq!(actual["agent_type"], "forge");
        assert_eq!(actual["agent_transcript_path"], "/tmp/sub.jsonl");
        assert_eq!(actual["stop_hook_active"], false);
        assert!(actual.get("last_assistant_message").is_none());
    }

    #[test]
    fn test_subagent_stop_payload_into_hook_input_payload() {
        let fixture = SubagentStopPayload {
            agent_id: "agent-2".to_string(),
            agent_type: "sage".to_string(),
            agent_transcript_path: std::path::PathBuf::from("/tmp/s.jsonl"),
            stop_hook_active: true,
            last_assistant_message: Some("done".to_string()),
        };
        let actual: HookInputPayload = fixture.into();
        match actual {
            HookInputPayload::SubagentStop {
                agent_id,
                agent_type,
                agent_transcript_path,
                stop_hook_active,
                last_assistant_message,
            } => {
                assert_eq!(agent_id, "agent-2");
                assert_eq!(agent_type, "sage");
                assert_eq!(
                    agent_transcript_path,
                    std::path::PathBuf::from("/tmp/s.jsonl")
                );
                assert!(stop_hook_active);
                assert_eq!(last_assistant_message.as_deref(), Some("done"));
            }
            other => panic!("expected SubagentStop wire variant, got {other:?}"),
        }
    }

    // ---- Permission payload tests ----

    #[test]
    fn test_permission_destination_serializes_as_camel_case() {
        assert_eq!(
            serde_json::to_string(&PermissionDestination::UserSettings).unwrap(),
            "\"userSettings\""
        );
        assert_eq!(
            serde_json::to_string(&PermissionDestination::ProjectSettings).unwrap(),
            "\"projectSettings\""
        );
        assert_eq!(
            serde_json::to_string(&PermissionDestination::LocalSettings).unwrap(),
            "\"localSettings\""
        );
        assert_eq!(
            serde_json::to_string(&PermissionDestination::Session).unwrap(),
            "\"session\""
        );
    }

    #[test]
    fn test_permission_update_serializes_with_camel_case_fields() {
        let fixture = PermissionUpdate {
            rules: vec!["Bash(git *)".to_string()],
            behavior: PermissionBehavior::Allow,
            destination: PermissionDestination::ProjectSettings,
        };
        let actual = serde_json::to_value(&fixture).unwrap();
        assert_eq!(actual["rules"][0], "Bash(git *)");
        assert_eq!(actual["behavior"], "allow");
        assert_eq!(actual["destination"], "projectSettings");
    }

    #[test]
    fn test_permission_request_payload_into_hook_input_payload() {
        let fixture = PermissionRequestPayload {
            tool_name: "Bash".to_string(),
            tool_input: json!({"command": "rm -rf /"}),
            permission_suggestions: vec![PermissionUpdate {
                rules: vec!["Bash(rm *)".to_string()],
                behavior: PermissionBehavior::Deny,
                destination: PermissionDestination::Session,
            }],
        };
        let actual: HookInputPayload = fixture.into();
        match actual {
            HookInputPayload::PermissionRequest {
                tool_name,
                tool_input,
                permission_suggestions,
            } => {
                assert_eq!(tool_name, "Bash");
                assert_eq!(tool_input["command"], "rm -rf /");
                assert_eq!(permission_suggestions.len(), 1);
                assert_eq!(permission_suggestions[0].behavior, PermissionBehavior::Deny);
            }
            other => panic!("expected PermissionRequest wire variant, got {other:?}"),
        }
    }

    #[test]
    fn test_permission_denied_payload_into_hook_input_payload() {
        let fixture = PermissionDeniedPayload {
            tool_name: "Write".to_string(),
            tool_input: json!({"path": "/etc/passwd"}),
            tool_use_id: "toolu_01".to_string(),
            reason: "policy violation".to_string(),
        };
        let actual: HookInputPayload = fixture.into();
        match actual {
            HookInputPayload::PermissionDenied { tool_name, tool_input, tool_use_id, reason } => {
                assert_eq!(tool_name, "Write");
                assert_eq!(tool_input["path"], "/etc/passwd");
                assert_eq!(tool_use_id, "toolu_01");
                assert_eq!(reason, "policy violation");
            }
            other => panic!("expected PermissionDenied wire variant, got {other:?}"),
        }
    }

    // ---- Cwd + FileChanged payload tests ----

    #[test]
    fn test_file_change_event_wire_str_all_variants() {
        assert_eq!(FileChangeEvent::Change.as_wire_str(), "change");
        assert_eq!(FileChangeEvent::Add.as_wire_str(), "add");
        assert_eq!(FileChangeEvent::Unlink.as_wire_str(), "unlink");
    }

    #[test]
    fn test_cwd_changed_payload_into_hook_input_payload() {
        let fixture = CwdChangedPayload {
            old_cwd: std::path::PathBuf::from("/home/a"),
            new_cwd: std::path::PathBuf::from("/home/a/project"),
        };
        let actual: HookInputPayload = fixture.into();
        match actual {
            HookInputPayload::CwdChanged { old_cwd, new_cwd } => {
                assert_eq!(old_cwd, std::path::PathBuf::from("/home/a"));
                assert_eq!(new_cwd, std::path::PathBuf::from("/home/a/project"));
            }
            other => panic!("expected CwdChanged wire variant, got {other:?}"),
        }
    }

    #[test]
    fn test_file_changed_payload_into_hook_input_payload_maps_event_string() {
        let fixture = FileChangedPayload {
            file_path: std::path::PathBuf::from("/tmp/x.txt"),
            event: FileChangeEvent::Unlink,
        };
        let actual: HookInputPayload = fixture.into();
        match actual {
            HookInputPayload::FileChanged { file_path, event } => {
                assert_eq!(file_path, std::path::PathBuf::from("/tmp/x.txt"));
                assert_eq!(event, "unlink");
            }
            other => panic!("expected FileChanged wire variant, got {other:?}"),
        }
    }

    // ---- Worktree payload tests ----

    #[test]
    fn test_worktree_create_payload_serializes_with_name_field() {
        let fixture = WorktreeCreatePayload { name: "feature-branch".to_string() };
        let json = serde_json::to_value(&fixture).unwrap();
        assert_eq!(json, json!({ "name": "feature-branch" }));
    }

    #[test]
    fn test_worktree_create_payload_into_hook_input_payload() {
        let fixture = WorktreeCreatePayload { name: "refactor-auth".to_string() };
        let actual: HookInputPayload = fixture.into();
        match actual {
            HookInputPayload::WorktreeCreate { name } => {
                assert_eq!(name, "refactor-auth");
            }
            other => panic!("expected WorktreeCreate wire variant, got {other:?}"),
        }
    }

    #[test]
    fn test_worktree_remove_payload_serializes_with_worktree_path_field() {
        let fixture =
            WorktreeRemovePayload { worktree_path: std::path::PathBuf::from("/tmp/wt/feature") };
        let json = serde_json::to_value(&fixture).unwrap();
        assert_eq!(json, json!({ "worktree_path": "/tmp/wt/feature" }));
    }

    #[test]
    fn test_worktree_remove_payload_into_hook_input_payload() {
        let fixture =
            WorktreeRemovePayload { worktree_path: std::path::PathBuf::from("/tmp/wt/feature") };
        let actual: HookInputPayload = fixture.into();
        match actual {
            HookInputPayload::WorktreeRemove { worktree_path } => {
                assert_eq!(worktree_path, std::path::PathBuf::from("/tmp/wt/feature"));
            }
            other => panic!("expected WorktreeRemove wire variant, got {other:?}"),
        }
    }

    // ---- InstructionsLoaded payload tests ----
    //
    // Wire-string coverage for [`MemoryType`] and [`InstructionsLoadReason`]
    // lives next to the type definitions in `crate::memory`; here we
    // only exercise the payload-to-wire conversion that is unique to
    // this file.

    #[test]
    fn test_instructions_loaded_payload_into_hook_input_payload() {
        let fixture = InstructionsLoadedPayload {
            file_path: std::path::PathBuf::from("/repo/AGENTS.md"),
            memory_type: MemoryType::Project,
            load_reason: InstructionsLoadReason::SessionStart,
            globs: Some(vec!["**/*.rs".to_string()]),
            trigger_file_path: None,
            parent_file_path: None,
        };
        let actual: HookInputPayload = fixture.into();
        match actual {
            HookInputPayload::InstructionsLoaded {
                file_path,
                memory_type,
                load_reason,
                globs,
                trigger_file_path,
                parent_file_path,
            } => {
                assert_eq!(file_path, std::path::PathBuf::from("/repo/AGENTS.md"));
                assert_eq!(memory_type, "project");
                assert_eq!(load_reason, "session_start");
                assert_eq!(globs.as_deref(), Some(&["**/*.rs".to_string()][..]));
                assert!(trigger_file_path.is_none());
                assert!(parent_file_path.is_none());
            }
            other => panic!("expected InstructionsLoaded wire variant, got {other:?}"),
        }
    }

    // ---- Elicitation payload tests ----

    #[test]
    fn test_elicitation_payload_into_hook_input_payload_form_mode() {
        let fixture = ElicitationPayload {
            server_name: "github".to_string(),
            message: "Provide a PR title".to_string(),
            requested_schema: Some(json!({
                "type": "object",
                "properties": {"title": {"type": "string"}}
            })),
            mode: Some("form".to_string()),
            url: None,
            elicitation_id: None,
        };
        let actual: HookInputPayload = fixture.into();
        match actual {
            HookInputPayload::Elicitation {
                server_name,
                message,
                requested_schema,
                mode,
                url,
                ..
            } => {
                assert_eq!(server_name, "github");
                assert_eq!(message, "Provide a PR title");
                assert!(requested_schema.is_some());
                assert_eq!(
                    requested_schema.unwrap()["properties"]["title"]["type"],
                    "string"
                );
                assert_eq!(mode.as_deref(), Some("form"));
                assert!(url.is_none());
            }
            other => panic!("expected Elicitation wire variant, got {other:?}"),
        }
    }

    #[test]
    fn test_elicitation_payload_into_hook_input_payload_url_mode() {
        let fixture = ElicitationPayload {
            server_name: "oauth-server".to_string(),
            message: "Open this link".to_string(),
            requested_schema: None,
            mode: Some("url".to_string()),
            url: Some("https://example.com/auth".to_string()),
            elicitation_id: None,
        };
        let actual: HookInputPayload = fixture.into();
        match actual {
            HookInputPayload::Elicitation { server_name, requested_schema, mode, url, .. } => {
                assert_eq!(server_name, "oauth-server");
                assert!(requested_schema.is_none());
                assert_eq!(mode.as_deref(), Some("url"));
                assert_eq!(url.as_deref(), Some("https://example.com/auth"));
            }
            other => panic!("expected Elicitation wire variant, got {other:?}"),
        }
    }

    #[test]
    fn test_elicitation_result_payload_into_hook_input_payload_accept() {
        let fixture = ElicitationResultPayload {
            server_name: "github".to_string(),
            action: "accept".to_string(),
            content: Some(json!({"title": "My PR"})),
            mode: None,
            elicitation_id: None,
        };
        let actual: HookInputPayload = fixture.into();
        match actual {
            HookInputPayload::ElicitationResult { server_name, action, content, .. } => {
                assert_eq!(server_name, "github");
                assert_eq!(action, "accept");
                assert_eq!(content.unwrap()["title"], "My PR");
            }
            other => panic!("expected ElicitationResult wire variant, got {other:?}"),
        }
    }
}
