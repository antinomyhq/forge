//! Lifecycle fire helpers for plugin hook events.
//!
//! This module hosts the out-of-orchestrator fire sites for
//! [`NotificationPayload`] and [`SetupPayload`]. Both helpers live in
//! `forge_app` (rather than `forge_services`) because they need direct
//! access to [`crate::hooks::PluginHookHandler`], which is crate-private
//! to `forge_app` through its private `hooks` module.
//!
//! The two entry points are:
//!
//! 1. [`ForgeNotificationService`] — concrete [`NotificationService`]
//!    implementation. Calling [`NotificationService::emit`] fires the
//!    `Notification` lifecycle event through the plugin hook dispatcher
//!    (observability only — hook errors never propagate) and, when the current
//!    stderr is a non-VS-Code TTY, emits a best-effort terminal bell so REPL
//!    users get a passive nudge.
//!
//! 2. [`fire_setup_hook`] — free function used by `ForgeAPI` to fire the
//!    `Setup` lifecycle event when the user invokes `forge --init` / `forge
//!    --maintenance`. Per Claude Code semantics (`hooksConfigManager.ts:175`)
//!    blocking errors from Setup hooks are intentionally discarded; the fire is
//!    observability-only.
//!
//! Both helpers construct a scratch [`Conversation`] because neither is
//! scoped to a live session — the orchestrator lifecycle isn't running
//! when a notification is emitted from the REPL prompt loop, and Setup
//! fires before any conversation has been initialized. The scratch
//! conversation is discarded immediately after the dispatch.

use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use forge_domain::{
    Agent, AggregatedHookResult, ConfigChangePayload, ConfigSource, Conversation, ConversationId,
    CwdChangedPayload, ElicitationPayload, ElicitationResultPayload, EventData, EventHandle,
    FileChangeEvent, FileChangedPayload, InstructionsLoadedPayload, LoadedInstructions, ModelId,
    NotificationPayload, PermissionDeniedPayload, PermissionRequestPayload, PermissionUpdate,
    SetupPayload, SetupTrigger, SubagentStartPayload, SubagentStopPayload, WorktreeCreatePayload,
    WorktreeRemovePayload,
};
use notify_debouncer_full::notify::RecursiveMode;
use tracing::{debug, warn};

use crate::hooks::PluginHookHandler;
use crate::services::{AgentRegistry, Notification, NotificationService, Services};

/// Resolve an [`Agent`] from the services registry.
///
/// Prefers the active agent, falling back to the first registered
/// agent. Returns `None` when the registry is empty — callers should
/// skip the hook fire entirely because the hook infrastructure requires
/// a non-`None` agent tag on every event.
async fn resolve_agent_from_services<S: Services>(services: &S) -> Option<Agent> {
    // Prefer the active agent.
    if let Ok(Some(active_id)) = services.get_active_agent_id().await
        && let Ok(Some(agent)) = services.get_agent(&active_id).await
    {
        return Some(agent);
    }

    // Fall back to any registered agent.
    services
        .get_agents()
        .await
        .ok()
        .and_then(|agents| agents.into_iter().next())
}

/// Runtime-settable accessor for the background
/// `FileChangedWatcher` used by the dynamic `watch_paths` wiring.
///
/// The orchestrator's `SessionStart` fire site needs to push
/// watch-path additions from a hook's
/// [`forge_domain::AggregatedHookResult::watch_paths`] back into the
/// running watcher, but `forge_app` cannot name the concrete
/// `FileChangedWatcherHandle` without creating a dependency cycle
/// (the handle lives in `forge_api`, which already depends on
/// `forge_app`). This trait gives `forge_app` a minimal, concrete-
/// handle-agnostic interface so the two crates fit together.
///
/// Implementations live in `forge_api::file_changed_watcher_handle`
/// and are registered with [`install_file_changed_watcher_ops`]
/// during `ForgeAPI::init`. The orchestrator later calls
/// [`add_file_changed_watch_paths`] from its `SessionStart`
/// aggregator — if no ops have been installed yet (e.g. in a unit
/// test that bypasses `ForgeAPI::init`), the call is a silent no-op.
pub trait FileChangedWatcherOps: Send + Sync {
    /// Install additional runtime watchers over the given paths.
    ///
    /// Implementations are responsible for splitting any pipe-
    /// separated hook matcher strings (e.g. `.envrc|.env`) into
    /// individual entries before calling this method — `watch_paths`
    /// here is expected to already be a flat list of absolute /
    /// cwd-resolved `(PathBuf, RecursiveMode)` pairs.
    fn add_paths(&self, watch_paths: Vec<(PathBuf, RecursiveMode)>);
}

/// Process-wide slot holding the runtime `FileChangedWatcher`
/// accessor. Populated exactly once by `ForgeAPI::init` via
/// [`install_file_changed_watcher_ops`]; read by the orchestrator's
/// `SessionStart` fire site via [`add_file_changed_watch_paths`].
///
/// This deliberately uses [`OnceLock`] rather than plumbing the
/// handle through every layer of the services stack: the watcher is
/// conceptually process-wide (there is one `ForgeAPI` per process),
/// it is installed before any orchestrator run, and the alternative —
/// adding a setter to the `Services` trait — would touch more than
/// a dozen crates for what is essentially a late-binding hook.
/// Mirrors the same pattern used by `ConfigWatcherHandle` in its own
/// `ForgeAPI::init` wiring.
static FILE_CHANGED_WATCHER_OPS: OnceLock<Arc<dyn FileChangedWatcherOps>> = OnceLock::new();

/// Register the live [`FileChangedWatcherOps`] implementation so the
/// orchestrator's `SessionStart` fire site can call
/// [`add_file_changed_watch_paths`] at runtime.
///
/// Called exactly once from `ForgeAPI::init` after
/// [`crate::file_changed_watcher_handle::FileChangedWatcherHandle::spawn`]
/// (in `forge_api`) succeeds. Subsequent calls are a silent no-op
/// because [`OnceLock::set`] returns `Err` on a second write — the
/// process-wide singleton is intentionally immutable.
///
/// # Test-harness behaviour
///
/// Unit tests that construct a `ForgeAPI` without a multi-threaded
/// tokio runtime never reach this installer, which is fine:
/// [`add_file_changed_watch_paths`] is a no-op when nothing has been
/// installed, so tests continue to run without needing to mock the
/// watcher.
pub fn install_file_changed_watcher_ops(ops: Arc<dyn FileChangedWatcherOps>) {
    if FILE_CHANGED_WATCHER_OPS.set(ops).is_err() {
        debug!(
            "install_file_changed_watcher_ops called twice; \
             ignoring the second install (OnceLock is already populated)"
        );
    }
}

/// Push runtime watch-path additions into the installed
/// [`FileChangedWatcherOps`] implementation.
///
/// Called by the orchestrator after a `SessionStart` hook returns
/// `watch_paths` in its [`forge_domain::AggregatedHookResult`]. If
/// no ops have been installed yet (e.g. in unit tests, or when
/// `ForgeAPI::init` degraded to a no-op watcher because no
/// multi-thread tokio runtime was active), this is a silent no-op —
/// dynamic watch_paths are observability-only and losing them is
/// never a correctness bug.
pub fn add_file_changed_watch_paths(watch_paths: Vec<(PathBuf, RecursiveMode)>) {
    if watch_paths.is_empty() {
        return;
    }
    if let Some(ops) = FILE_CHANGED_WATCHER_OPS.get() {
        ops.add_paths(watch_paths);
    } else {
        debug!(
            "add_file_changed_watch_paths called before \
             install_file_changed_watcher_ops — dropping runtime watch paths \
             (expected in unit tests that bypass ForgeAPI::init)"
        );
    }
}

/// Production implementation of [`NotificationService`].
///
/// Cheap to construct — holds only an `Arc<S>` to the services aggregate.
/// Construct one per call from the API layer; there is no persistent
/// state to cache.
pub struct ForgeNotificationService<S> {
    services: Arc<S>,
}

impl<S> ForgeNotificationService<S> {
    /// Create a new service backed by the given [`Services`] handle.
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }
}

impl<S: Services> ForgeNotificationService<S> {
    /// Returns `true` if stderr is a TTY and we are **not** inside a VS
    /// Code integrated terminal.
    ///
    /// VS Code's integrated terminal forwards `\x07` as a loud modal
    /// alert, which is exactly the kind of disruption this function
    /// exists to avoid. The detection matches
    /// `crates/forge_main/src/vscode.rs:9-16` verbatim (duplicated here
    /// so `forge_app` does not need to depend on `forge_main`).
    fn should_beep() -> bool {
        if !io::stderr().is_terminal() {
            return false;
        }
        let in_vscode = std::env::var("TERM_PROGRAM")
            .map(|v| v == "vscode")
            .unwrap_or(false)
            || std::env::var("VSCODE_PID").is_ok()
            || std::env::var("VSCODE_GIT_ASKPASS_NODE").is_ok()
            || std::env::var("VSCODE_GIT_IPC_HANDLE").is_ok();
        !in_vscode
    }

    /// Best-effort BEL emission to stderr. Swallows IO errors — the bell
    /// is a nice-to-have and should never fail the caller.
    fn emit_bell() {
        let mut err = io::stderr();
        let _ = err.write_all(b"\x07");
        let _ = err.flush();
    }

    /// Look up an [`Agent`] to attach to the hook event. Delegates to
    /// [`resolve_agent_from_services`].
    async fn resolve_agent(&self) -> Option<Agent> {
        resolve_agent_from_services(self.services.as_ref()).await
    }
}

#[async_trait]
impl<S: Services> NotificationService for ForgeNotificationService<S> {
    async fn emit(&self, notification: Notification) -> anyhow::Result<()> {
        debug!(
            kind = ?notification.kind,
            title = ?notification.title,
            message = %notification.message,
            "emit notification"
        );

        // 1. Fire the Notification hook. Per the trait docs, hook dispatcher errors are
        //    soft failures: log and continue.
        if let Err(err) = self.fire_hook(&notification).await {
            warn!(error = %err, "failed to fire Notification hook");
        }

        // 2. Best-effort terminal bell.
        if Self::should_beep() {
            Self::emit_bell();
        }

        Ok(())
    }
}

impl<S: Services> ForgeNotificationService<S> {
    /// Dispatches the Notification lifecycle event through
    /// [`PluginHookHandler`]. The aggregated result is intentionally
    /// discarded — Notification is an observability-only event per the
    /// trait documentation in `services.rs:538-540`.
    async fn fire_hook(&self, notification: &Notification) -> anyhow::Result<()> {
        let Some(agent) = self.resolve_agent().await else {
            debug!("no agent available — skipping Notification hook fire");
            return Ok(());
        };
        let model_id: ModelId = agent.model.clone();

        let environment = self.services.get_environment();
        // Scratch conversation — Notification fires out-of-band (e.g. on
        // REPL idle) so there is no live Conversation to update. The
        // resulting hook_result is drained and discarded below.
        let mut scratch = Conversation::new(ConversationId::generate());
        let session_id = scratch.id.into_string();
        let transcript_path = environment.transcript_path(&session_id);
        let cwd = environment.cwd.clone();

        let payload = NotificationPayload {
            message: notification.message.clone(),
            title: notification.title.clone(),
            notification_type: notification.kind.as_wire_str().to_string(),
        };

        let event =
            EventData::with_context(agent, model_id, session_id, transcript_path, cwd, payload);

        let plugin_handler = PluginHookHandler::new(self.services.clone());
        <PluginHookHandler<S> as EventHandle<EventData<NotificationPayload>>>::handle(
            &plugin_handler,
            &event,
            &mut scratch,
        )
        .await?;

        // Drain and discard the hook_result — Notification is
        // observability only, blocking_error does not apply.
        let _ = std::mem::take(&mut scratch.hook_result);
        Ok(())
    }
}

/// Fire the `Setup` lifecycle event with the given trigger.
///
/// Used by `ForgeAPI::fire_setup_hook` as the out-of-orchestrator entry
/// point for the `--init` / `--init-only` / `--maintenance` CLI flags.
/// Per Claude Code semantics (`hooksConfigManager.ts:175`) any blocking
/// error returned by a Setup hook is intentionally **discarded** — Setup
/// runs before a conversation exists, so there is nothing to block.
///
/// This function is safe to call even when no plugins are configured:
/// the hook dispatcher returns an empty result which is then drained.
pub async fn fire_setup_hook<S: Services>(
    services: Arc<S>,
    trigger: SetupTrigger,
) -> anyhow::Result<()> {
    let Some(agent) = resolve_agent_from_services(services.as_ref()).await else {
        debug!("no agent available — skipping Setup hook fire");
        return Ok(());
    };
    let model_id: ModelId = agent.model.clone();

    let environment = services.get_environment();
    let mut scratch = Conversation::new(ConversationId::generate());
    let session_id = scratch.id.into_string();
    let transcript_path = environment.transcript_path(&session_id);
    let cwd = environment.cwd.clone();

    let payload = SetupPayload { trigger };
    let event = EventData::with_context(agent, model_id, session_id, transcript_path, cwd, payload);

    let plugin_handler = PluginHookHandler::new(services.clone());
    <PluginHookHandler<S> as EventHandle<EventData<SetupPayload>>>::handle(
        &plugin_handler,
        &event,
        &mut scratch,
    )
    .await?;

    // Drain and explicitly ignore the blocking_error per Claude Code
    // semantics (setup hooks cannot block — they run before any
    // conversation exists).
    let aggregated = std::mem::take(&mut scratch.hook_result);
    if let Some(err) = aggregated.blocking_error {
        debug!(
            trigger = ?trigger,
            error = %err.message,
            "Setup hook returned blocking_error; ignoring per Claude Code semantics"
        );
    }

    Ok(())
}

/// Fire the `ConfigChange` lifecycle event for a debounced config
/// file/directory change.
///
/// Used by `ForgeAPI` as the out-of-orchestrator entry point for the
/// `ConfigWatcher` service. The watcher hands us a
/// classified [`ConfigSource`] and absolute `file_path`; we wrap them
/// in a [`ConfigChangePayload`] and dispatch through
/// [`PluginHookHandler`] on a scratch [`Conversation`].
///
/// Per the trait documentation in `services.rs:538-540`, ConfigChange
/// is an observability-only event — hook dispatcher errors are soft
/// failures (logged at `warn!`) and any `blocking_error` on the
/// aggregated result is drained and discarded. Config changes can
/// fire at any time (including from a background watcher thread),
/// long after the triggering conversation is gone, so there is
/// nothing to block.
///
/// This function is safe to call even when no plugins are configured:
/// the hook dispatcher returns an empty result which is then drained.
pub async fn fire_config_change_hook<S: Services>(
    services: Arc<S>,
    source: ConfigSource,
    file_path: Option<PathBuf>,
) {
    let Some(agent) = resolve_agent_from_services(services.as_ref()).await else {
        debug!("no agent available — skipping ConfigChange hook fire");
        return;
    };
    let model_id: ModelId = agent.model.clone();

    let environment = services.get_environment();
    let mut scratch = Conversation::new(ConversationId::generate());
    let session_id = scratch.id.into_string();
    let transcript_path = environment.transcript_path(&session_id);
    let cwd = environment.cwd.clone();

    let payload = ConfigChangePayload { source, file_path };
    let event = EventData::with_context(agent, model_id, session_id, transcript_path, cwd, payload);

    let plugin_handler = PluginHookHandler::new(services.clone());
    if let Err(err) = <PluginHookHandler<S> as EventHandle<EventData<ConfigChangePayload>>>::handle(
        &plugin_handler,
        &event,
        &mut scratch,
    )
    .await
    {
        warn!(
            source = ?source,
            error = %err,
            "failed to dispatch ConfigChange hook; ignoring per Claude Code semantics"
        );
    }

    // Drain and explicitly ignore the blocking_error. ConfigChange is
    // observability-only — the watcher callback runs asynchronously
    // on a background thread with no conversation to block against.
    let aggregated = std::mem::take(&mut scratch.hook_result);
    if let Some(err) = aggregated.blocking_error {
        debug!(
            source = ?source,
            error = %err.message,
            "ConfigChange hook returned blocking_error; ignoring (observability only)"
        );
    }
}

/// Fire the `FileChanged` lifecycle event for a debounced filesystem
/// change under one of the user's watched paths.
///
/// Used by `ForgeAPI` as the out-of-orchestrator entry point for the
/// `FileChangedWatcher` service. The watcher hands us an
/// absolute `file_path` and a [`FileChangeEvent`] discriminator; we
/// wrap them in a [`FileChangedPayload`] and dispatch through
/// [`PluginHookHandler`] on a scratch [`Conversation`].
///
/// Per Claude Code's `FileChanged` semantics, the event is
/// **observability-only** — any `blocking_error` returned by a
/// plugin hook is drained and discarded, and dispatch failures are
/// logged at `warn!` but never propagated. Dynamic extension of the
/// watched-paths set based on hook results is pending.
///
/// This function is safe to call even when no plugins are configured:
/// the hook dispatcher returns an empty result which is then drained.
pub async fn fire_file_changed_hook<S: Services>(
    services: Arc<S>,
    file_path: PathBuf,
    event: FileChangeEvent,
) {
    let Some(agent) = resolve_agent_from_services(services.as_ref()).await else {
        debug!("no agent available — skipping FileChanged hook fire");
        return;
    };
    let model_id: ModelId = agent.model.clone();

    let environment = services.get_environment();
    let mut scratch = Conversation::new(ConversationId::generate());
    let session_id = scratch.id.into_string();
    let transcript_path = environment.transcript_path(&session_id);
    let cwd = environment.cwd.clone();

    let payload = FileChangedPayload { file_path: file_path.clone(), event };
    let event_data =
        EventData::with_context(agent, model_id, session_id, transcript_path, cwd, payload);

    let plugin_handler = PluginHookHandler::new(services.clone());
    if let Err(err) = <PluginHookHandler<S> as EventHandle<EventData<FileChangedPayload>>>::handle(
        &plugin_handler,
        &event_data,
        &mut scratch,
    )
    .await
    {
        warn!(
            path = %file_path.display(),
            event = ?event,
            error = %err,
            "failed to dispatch FileChanged hook; ignoring per Claude Code semantics"
        );
    }

    // Drain and explicitly ignore the blocking_error. FileChanged is
    // observability-only — the watcher callback runs asynchronously
    // on a background thread with no conversation to block against.
    // Dynamic watch-path extension based on hook results is pending.
    let aggregated = std::mem::take(&mut scratch.hook_result);
    if let Some(err) = aggregated.blocking_error {
        debug!(
            path = %file_path.display(),
            event = ?event,
            error = %err.message,
            "FileChanged hook returned blocking_error; ignoring (observability only)"
        );
    }
}

/// Fire the `InstructionsLoaded` lifecycle event for a single
/// instructions file that was just loaded into the agent's context.
///
/// Used by `ForgeApp::chat` to dispatch one hook event per AGENTS.md
/// file returned by
/// [`crate::CustomInstructionsService::get_custom_instructions_detailed`].
/// Currently fires with
/// [`forge_domain::InstructionsLoadReason::SessionStart`]; the nested
/// traversal, conditional-rule, `@include` and post-compact reasons
/// are pending.
///
/// Per Claude Code semantics, `InstructionsLoaded` is an
/// **observability-only** event — any `blocking_error` returned by a
/// plugin hook is drained and discarded, and dispatch failures are
/// logged at `warn!` but never propagated to the caller. The memory
/// layer cannot veto a load of its own source files.
///
/// This function is safe to call even when no plugins are configured:
/// the hook dispatcher returns an empty result which is then drained.
pub async fn fire_instructions_loaded_hook<S: Services>(
    services: Arc<S>,
    loaded: LoadedInstructions,
) {
    let Some(agent) = resolve_agent_from_services(services.as_ref()).await else {
        debug!("no agent available — skipping InstructionsLoaded hook fire");
        return;
    };
    let model_id: ModelId = agent.model.clone();

    let environment = services.get_environment();
    let mut scratch = Conversation::new(ConversationId::generate());
    let session_id = scratch.id.into_string();
    let transcript_path = environment.transcript_path(&session_id);
    let cwd = environment.cwd.clone();

    // Project the LoadedInstructions into the wire payload. The
    // payload struct uses the typed enums directly (not strings), so
    // we pass `memory_type` / `load_reason` verbatim.
    let payload = InstructionsLoadedPayload {
        file_path: loaded.file_path,
        memory_type: loaded.memory_type,
        load_reason: loaded.load_reason,
        globs: loaded.globs,
        trigger_file_path: loaded.trigger_file_path,
        parent_file_path: loaded.parent_file_path,
    };

    let event = EventData::with_context(agent, model_id, session_id, transcript_path, cwd, payload);

    let plugin_handler = PluginHookHandler::new(services.clone());
    if let Err(err) =
        <PluginHookHandler<S> as EventHandle<EventData<InstructionsLoadedPayload>>>::handle(
            &plugin_handler,
            &event,
            &mut scratch,
        )
        .await
    {
        warn!(
            error = %err,
            "failed to dispatch InstructionsLoaded hook; ignoring per Claude Code semantics"
        );
    }

    // Drain and explicitly ignore the blocking_error — InstructionsLoaded
    // is observability-only. The memory layer cannot be vetoed by a
    // plugin.
    let aggregated = std::mem::take(&mut scratch.hook_result);
    if let Some(err) = aggregated.blocking_error {
        debug!(
            error = %err.message,
            "InstructionsLoaded hook returned blocking_error; ignoring (observability only)"
        );
    }
}

/// Fire the `WorktreeCreate` lifecycle event with the given worktree
/// name and return the aggregated hook result.
///
/// Used by `crates/forge_main/src/sandbox.rs` as the out-of-orchestrator
/// entry point for the `--worktree` CLI flag. Unlike the other fire
/// helpers in this module (which discard the aggregated result because
/// their events are observability-only), this one **returns** the
/// aggregate so the caller can consume:
///
/// - `worktree_path` — a plugin-provided path override that the caller should
///   use instead of running `git worktree add`.
/// - `blocking_error` — a plugin veto of the worktree creation altogether. The
///   caller is expected to surface this as an error.
/// - `additional_contexts` / `system_messages` — pre-creation reminders that a
///   future runtime `EnterWorktreeTool` fire site can forward into the
///   conversation.
///
/// Dispatch failures are handled fail-open: any error from the hook
/// plumbing is logged at `tracing::warn` and an empty
/// `AggregatedHookResult` is returned, so the caller falls back to the
/// built-in `git worktree add` path. This matches the observability-
/// over-correctness philosophy of the other fire sites.
pub async fn fire_worktree_create_hook<S: Services>(
    services: Arc<S>,
    name: String,
) -> AggregatedHookResult {
    let Some(agent) = resolve_agent_from_services(services.as_ref()).await else {
        debug!("no agent available — skipping WorktreeCreate hook fire");
        return AggregatedHookResult::default();
    };
    let model_id: ModelId = agent.model.clone();

    let environment = services.get_environment();
    // Scratch conversation — WorktreeCreate fires from the CLI
    // `--worktree` flag handler, which runs before the live
    // orchestrator has been set up. The scratch conversation is
    // dropped as soon as we drain its `hook_result` below.
    let mut scratch = Conversation::new(ConversationId::generate());
    let session_id = scratch.id.into_string();
    let transcript_path = environment.transcript_path(&session_id);
    let cwd = environment.cwd.clone();

    let payload = WorktreeCreatePayload { name: name.clone() };
    let event = EventData::with_context(agent, model_id, session_id, transcript_path, cwd, payload);

    let plugin_handler = PluginHookHandler::new(services.clone());
    if let Err(err) =
        <PluginHookHandler<S> as EventHandle<EventData<WorktreeCreatePayload>>>::handle(
            &plugin_handler,
            &event,
            &mut scratch,
        )
        .await
    {
        warn!(
            name = %name,
            error = %err,
            "failed to dispatch WorktreeCreate hook; falling back to built-in git worktree add"
        );
        return AggregatedHookResult::default();
    }

    // Drain the aggregated result so the caller can inspect
    // worktree_path / blocking_error / additional_contexts. The
    // scratch conversation itself is dropped at the end of the
    // function scope.
    std::mem::take(&mut scratch.hook_result)
}

/// Fires the `Elicitation` plugin hook with the given payload data.
///
/// Returns the [`AggregatedHookResult`] so the caller (the MCP
/// `ElicitationDispatcher`) can consume:
///
/// - `blocking_error` → cancel the elicitation with an error message.
/// - `permission_behavior == Allow` + `updated_input` → auto-accept with the
///   plugin-provided form data (the `updated_input` value is the `content`
///   field of the MCP response).
/// - `permission_behavior == Deny` → decline without prompting the user.
///
/// Fail-open on dispatch errors: logs via `tracing::warn` and returns
/// [`AggregatedHookResult::default`] so the dispatcher falls through to
/// the interactive UI fallback.
pub async fn fire_elicitation_hook<S: Services>(
    services: Arc<S>,
    server_name: String,
    message: String,
    requested_schema: Option<serde_json::Value>,
    mode: Option<String>,
    url: Option<String>,
) -> AggregatedHookResult {
    let Some(agent) = resolve_agent_from_services(services.as_ref()).await else {
        debug!("no agent available — skipping Elicitation hook fire");
        return AggregatedHookResult::default();
    };
    let model_id: ModelId = agent.model.clone();

    let environment = services.get_environment();
    let mut scratch = Conversation::new(ConversationId::generate());
    let session_id = scratch.id.into_string();
    let transcript_path = environment.transcript_path(&session_id);
    let cwd = environment.cwd.clone();

    let payload = ElicitationPayload {
        server_name: server_name.clone(),
        message,
        requested_schema,
        mode,
        url,
        elicitation_id: None,
    };
    let event = EventData::with_context(agent, model_id, session_id, transcript_path, cwd, payload);

    let plugin_handler = PluginHookHandler::new(services.clone());
    if let Err(err) = <PluginHookHandler<S> as EventHandle<EventData<ElicitationPayload>>>::handle(
        &plugin_handler,
        &event,
        &mut scratch,
    )
    .await
    {
        warn!(
            server_name = %server_name,
            error = %err,
            "failed to dispatch Elicitation hook; falling back to interactive UI path"
        );
        return AggregatedHookResult::default();
    }

    // Drain the aggregated result so the caller can inspect
    // blocking_error / permission_behavior / updated_input. The
    // scratch conversation itself is dropped at the end of the
    // function scope.
    std::mem::take(&mut scratch.hook_result)
}

/// Fires the `ElicitationResult` plugin hook after the user (or an
/// auto-responding plugin hook) has completed an elicitation request.
///
/// This is fire-and-forget — the aggregated result is drained and
/// discarded per the observability-only contract. Plugins use this
/// event for audit logging, analytics, or follow-up actions after an
/// elicitation completes.
///
/// Fail-open on dispatch errors: logs via `tracing::warn` and returns
/// without propagating so the MCP response path is never blocked by a
/// misbehaving plugin.
pub async fn fire_elicitation_result_hook<S: Services>(
    services: Arc<S>,
    server_name: String,
    action: String,
    content: Option<serde_json::Value>,
) {
    let Some(agent) = resolve_agent_from_services(services.as_ref()).await else {
        debug!("no agent available — skipping ElicitationResult hook fire");
        return;
    };
    let model_id: ModelId = agent.model.clone();

    let environment = services.get_environment();
    let mut scratch = Conversation::new(ConversationId::generate());
    let session_id = scratch.id.into_string();
    let transcript_path = environment.transcript_path(&session_id);
    let cwd = environment.cwd.clone();

    let payload = ElicitationResultPayload {
        server_name: server_name.clone(),
        action,
        content,
        mode: None,
        elicitation_id: None,
    };
    let event = EventData::with_context(agent, model_id, session_id, transcript_path, cwd, payload);

    let plugin_handler = PluginHookHandler::new(services.clone());
    if let Err(err) =
        <PluginHookHandler<S> as EventHandle<EventData<ElicitationResultPayload>>>::handle(
            &plugin_handler,
            &event,
            &mut scratch,
        )
        .await
    {
        warn!(
            server_name = %server_name,
            error = %err,
            "failed to dispatch ElicitationResult hook (observability-only, ignoring)"
        );
        return;
    }

    // ElicitationResult is observability-only; drain the aggregated
    // result and discard it. Plugins cannot block or modify the
    // response via this event.
    let _ = std::mem::take(&mut scratch.hook_result);
}

/// Fire the `SubagentStart` lifecycle event when a sub-agent (Task tool)
/// begins execution.
///
/// Used by the orchestrator's sub-agent spawning path to notify plugin
/// hooks that a new sub-agent has started. Per Claude Code semantics,
/// `SubagentStart` is an **observability-only** event — any
/// `blocking_error` returned by a plugin hook is drained and discarded,
/// and dispatch failures are logged at `warn!` but never propagated.
///
/// This function is safe to call even when no plugins are configured:
/// the hook dispatcher returns an empty result which is then drained.
pub async fn fire_subagent_start_hook<S: Services>(
    services: Arc<S>,
    agent_id: String,
    agent_type: String,
) {
    let Some(agent) = resolve_agent_from_services(services.as_ref()).await else {
        debug!("no agent available — skipping SubagentStart hook fire");
        return;
    };
    let model_id: ModelId = agent.model.clone();

    let environment = services.get_environment();
    let mut scratch = Conversation::new(ConversationId::generate());
    let session_id = scratch.id.into_string();
    let transcript_path = environment.transcript_path(&session_id);
    let cwd = environment.cwd.clone();

    let payload =
        SubagentStartPayload { agent_id: agent_id.clone(), agent_type: agent_type.clone() };
    let event = EventData::with_context(agent, model_id, session_id, transcript_path, cwd, payload);

    let plugin_handler = PluginHookHandler::new(services.clone());
    if let Err(err) =
        <PluginHookHandler<S> as EventHandle<EventData<SubagentStartPayload>>>::handle(
            &plugin_handler,
            &event,
            &mut scratch,
        )
        .await
    {
        warn!(
            agent_id = %agent_id,
            agent_type = %agent_type,
            error = %err,
            "failed to dispatch SubagentStart hook; ignoring per Claude Code semantics"
        );
    }

    // Drain and explicitly ignore the blocking_error — SubagentStart is
    // observability-only.
    let aggregated = std::mem::take(&mut scratch.hook_result);
    if let Some(err) = aggregated.blocking_error {
        debug!(
            agent_id = %agent_id,
            agent_type = %agent_type,
            error = %err.message,
            "SubagentStart hook returned blocking_error; ignoring (observability only)"
        );
    }
}

/// Fire the `SubagentStop` lifecycle event when a sub-agent finishes
/// execution.
///
/// Used by the orchestrator's sub-agent completion path to notify plugin
/// hooks that a sub-agent has stopped. Per Claude Code semantics,
/// `SubagentStop` is an **observability-only** event — any
/// `blocking_error` is drained and discarded.
///
/// This function is safe to call even when no plugins are configured:
/// the hook dispatcher returns an empty result which is then drained.
pub async fn fire_subagent_stop_hook<S: Services>(
    services: Arc<S>,
    agent_id: String,
    agent_type: String,
    agent_transcript_path: PathBuf,
    stop_hook_active: bool,
    last_assistant_message: Option<String>,
) {
    let Some(agent) = resolve_agent_from_services(services.as_ref()).await else {
        debug!("no agent available — skipping SubagentStop hook fire");
        return;
    };
    let model_id: ModelId = agent.model.clone();

    let environment = services.get_environment();
    let mut scratch = Conversation::new(ConversationId::generate());
    let session_id = scratch.id.into_string();
    let transcript_path = environment.transcript_path(&session_id);
    let cwd = environment.cwd.clone();

    let payload = SubagentStopPayload {
        agent_id: agent_id.clone(),
        agent_type: agent_type.clone(),
        agent_transcript_path,
        stop_hook_active,
        last_assistant_message,
    };
    let event = EventData::with_context(agent, model_id, session_id, transcript_path, cwd, payload);

    let plugin_handler = PluginHookHandler::new(services.clone());
    if let Err(err) = <PluginHookHandler<S> as EventHandle<EventData<SubagentStopPayload>>>::handle(
        &plugin_handler,
        &event,
        &mut scratch,
    )
    .await
    {
        warn!(
            agent_id = %agent_id,
            agent_type = %agent_type,
            error = %err,
            "failed to dispatch SubagentStop hook; ignoring per Claude Code semantics"
        );
    }

    // Drain and explicitly ignore the blocking_error — SubagentStop is
    // observability-only.
    let aggregated = std::mem::take(&mut scratch.hook_result);
    if let Some(err) = aggregated.blocking_error {
        debug!(
            agent_id = %agent_id,
            agent_type = %agent_type,
            error = %err.message,
            "SubagentStop hook returned blocking_error; ignoring (observability only)"
        );
    }
}

/// Fire the `PermissionRequest` lifecycle event when the policy engine
/// encounters a tool call that requires permission.
///
/// Returns the [`AggregatedHookResult`] so the caller (the policy
/// engine) can consume:
///
/// - `permission_behavior == Allow` → auto-grant permission.
/// - `permission_behavior == Deny` → deny without prompting.
/// - `blocking_error` → surface an error to the orchestrator.
///
/// Fail-open on dispatch errors: logs via `tracing::warn` and returns
/// [`AggregatedHookResult::default`] so the policy engine falls through
/// to the interactive permission prompt.
pub async fn fire_permission_request_hook<S: Services>(
    services: Arc<S>,
    tool_name: String,
    tool_input: serde_json::Value,
    permission_suggestions: Vec<PermissionUpdate>,
) -> AggregatedHookResult {
    let Some(agent) = resolve_agent_from_services(services.as_ref()).await else {
        debug!("no agent available — skipping PermissionRequest hook fire");
        return AggregatedHookResult::default();
    };
    let model_id: ModelId = agent.model.clone();

    let environment = services.get_environment();
    let mut scratch = Conversation::new(ConversationId::generate());
    let session_id = scratch.id.into_string();
    let transcript_path = environment.transcript_path(&session_id);
    let cwd = environment.cwd.clone();

    let payload = PermissionRequestPayload {
        tool_name: tool_name.clone(),
        tool_input,
        permission_suggestions,
    };
    let event = EventData::with_context(agent, model_id, session_id, transcript_path, cwd, payload);

    let plugin_handler = PluginHookHandler::new(services.clone());
    if let Err(err) =
        <PluginHookHandler<S> as EventHandle<EventData<PermissionRequestPayload>>>::handle(
            &plugin_handler,
            &event,
            &mut scratch,
        )
        .await
    {
        warn!(
            tool_name = %tool_name,
            error = %err,
            "failed to dispatch PermissionRequest hook; falling back to interactive prompt"
        );
        return AggregatedHookResult::default();
    }

    // Drain the aggregated result so the caller can inspect
    // permission_behavior / blocking_error. The scratch conversation
    // itself is dropped at the end of the function scope.
    std::mem::take(&mut scratch.hook_result)
}

/// Fire the `PermissionDenied` lifecycle event after a permission
/// request is rejected.
///
/// This is fire-and-forget — the aggregated result is drained and
/// discarded per the observability-only contract. Plugins use this
/// event for audit logging or analytics.
///
/// Fail-open on dispatch errors: logs via `tracing::warn` and returns
/// without propagating so the denial path is never blocked by a
/// misbehaving plugin.
pub async fn fire_permission_denied_hook<S: Services>(
    services: Arc<S>,
    tool_name: String,
    tool_input: serde_json::Value,
    tool_use_id: String,
    reason: String,
) {
    let Some(agent) = resolve_agent_from_services(services.as_ref()).await else {
        debug!("no agent available — skipping PermissionDenied hook fire");
        return;
    };
    let model_id: ModelId = agent.model.clone();

    let environment = services.get_environment();
    let mut scratch = Conversation::new(ConversationId::generate());
    let session_id = scratch.id.into_string();
    let transcript_path = environment.transcript_path(&session_id);
    let cwd = environment.cwd.clone();

    let payload = PermissionDeniedPayload {
        tool_name: tool_name.clone(),
        tool_input,
        tool_use_id,
        reason,
    };
    let event = EventData::with_context(agent, model_id, session_id, transcript_path, cwd, payload);

    let plugin_handler = PluginHookHandler::new(services.clone());
    if let Err(err) =
        <PluginHookHandler<S> as EventHandle<EventData<PermissionDeniedPayload>>>::handle(
            &plugin_handler,
            &event,
            &mut scratch,
        )
        .await
    {
        warn!(
            tool_name = %tool_name,
            error = %err,
            "failed to dispatch PermissionDenied hook (observability-only, ignoring)"
        );
        return;
    }

    // PermissionDenied is observability-only; drain the aggregated
    // result and discard it.
    let aggregated = std::mem::take(&mut scratch.hook_result);
    if let Some(err) = aggregated.blocking_error {
        debug!(
            tool_name = %tool_name,
            error = %err.message,
            "PermissionDenied hook returned blocking_error; ignoring (observability only)"
        );
    }
}

/// Fire the `CwdChanged` lifecycle event when the orchestrator's
/// working directory changes.
///
/// Used by the Shell tool's cwd tracking to notify plugin hooks when
/// a `cd` command or worktree switch changes the effective working
/// directory. Per Claude Code semantics, `CwdChanged` is an
/// **observability-only** event — any `blocking_error` is drained and
/// discarded.
///
/// This function is safe to call even when no plugins are configured:
/// the hook dispatcher returns an empty result which is then drained.
pub async fn fire_cwd_changed_hook<S: Services>(
    services: Arc<S>,
    old_cwd: PathBuf,
    new_cwd: PathBuf,
) {
    let Some(agent) = resolve_agent_from_services(services.as_ref()).await else {
        debug!("no agent available — skipping CwdChanged hook fire");
        return;
    };
    let model_id: ModelId = agent.model.clone();

    let environment = services.get_environment();
    let mut scratch = Conversation::new(ConversationId::generate());
    let session_id = scratch.id.into_string();
    let transcript_path = environment.transcript_path(&session_id);
    let cwd = environment.cwd.clone();

    let payload = CwdChangedPayload { old_cwd: old_cwd.clone(), new_cwd: new_cwd.clone() };
    let event = EventData::with_context(agent, model_id, session_id, transcript_path, cwd, payload);

    let plugin_handler = PluginHookHandler::new(services.clone());
    if let Err(err) = <PluginHookHandler<S> as EventHandle<EventData<CwdChangedPayload>>>::handle(
        &plugin_handler,
        &event,
        &mut scratch,
    )
    .await
    {
        warn!(
            old_cwd = %old_cwd.display(),
            new_cwd = %new_cwd.display(),
            error = %err,
            "failed to dispatch CwdChanged hook; ignoring per Claude Code semantics"
        );
    }

    // Drain and explicitly ignore the blocking_error — CwdChanged is
    // observability-only.
    let aggregated = std::mem::take(&mut scratch.hook_result);
    if let Some(err) = aggregated.blocking_error {
        debug!(
            old_cwd = %old_cwd.display(),
            new_cwd = %new_cwd.display(),
            error = %err.message,
            "CwdChanged hook returned blocking_error; ignoring (observability only)"
        );
    }
}

/// Fire the `WorktreeRemove` lifecycle event when a worktree is
/// cleaned up.
///
/// Used by the worktree / sandbox cleanup path to notify plugin hooks
/// that a worktree has been removed. Returns the
/// [`AggregatedHookResult`] so the caller can consume any
/// `blocking_error` (a plugin veto of the removal) or
/// `additional_contexts` / `system_messages`.
///
/// Fail-open on dispatch errors: logs via `tracing::warn` and returns
/// [`AggregatedHookResult::default`] so the built-in `git worktree
/// remove` path proceeds.
pub async fn fire_worktree_remove_hook<S: Services>(
    services: Arc<S>,
    worktree_path: PathBuf,
) -> AggregatedHookResult {
    let Some(agent) = resolve_agent_from_services(services.as_ref()).await else {
        debug!("no agent available — skipping WorktreeRemove hook fire");
        return AggregatedHookResult::default();
    };
    let model_id: ModelId = agent.model.clone();

    let environment = services.get_environment();
    let mut scratch = Conversation::new(ConversationId::generate());
    let session_id = scratch.id.into_string();
    let transcript_path = environment.transcript_path(&session_id);
    let cwd = environment.cwd.clone();

    let payload = WorktreeRemovePayload { worktree_path: worktree_path.clone() };
    let event = EventData::with_context(agent, model_id, session_id, transcript_path, cwd, payload);

    let plugin_handler = PluginHookHandler::new(services.clone());
    if let Err(err) =
        <PluginHookHandler<S> as EventHandle<EventData<WorktreeRemovePayload>>>::handle(
            &plugin_handler,
            &event,
            &mut scratch,
        )
        .await
    {
        warn!(
            worktree_path = %worktree_path.display(),
            error = %err,
            "failed to dispatch WorktreeRemove hook; falling back to built-in git worktree remove"
        );
        return AggregatedHookResult::default();
    }

    // Drain the aggregated result so the caller can inspect
    // blocking_error / worktree_path override. The scratch
    // conversation itself is dropped at the end of the function scope.
    std::mem::take(&mut scratch.hook_result)
}

#[cfg(test)]
mod tests {
    // End-to-end dispatch behaviour for Notification and Setup is already
    // covered by the existing integration tests in
    // `crates/forge_app/src/hooks/plugin.rs`:
    //
    //   - `test_dispatch_notification_matches_notification_type`
    //   - `test_dispatch_setup_matches_trigger_string`
    //
    // Those tests exercise the same `PluginHookHandler` dispatcher that
    // `ForgeNotificationService` and `fire_setup_hook` call into, so we
    // rely on them for correctness.
    //
    // Unit tests for `should_beep` are intentionally omitted: the
    // detection reads env vars, which cannot be safely toggled from a
    // parallel test runner without serializing test threads. The
    // detection logic is a near-verbatim copy of the already-tested
    // `forge_main::vscode::is_vscode_terminal` function
    // (see `crates/forge_main/src/vscode.rs:86-110`).
}
