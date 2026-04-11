//! Plugin hook dispatcher.
//!
//! [`PluginHookHandler`] is the top-level dispatch entry point for every
//! lifecycle event that can fire a user/project/plugin-authored hook.
//! It consumes the merged [`MergedHooksConfig`] produced by
//! [`HookConfigLoaderService`], filters matching entries for the
//! requested event, runs every surviving hook through
//! [`HookExecutorInfra`] in parallel, and folds the results into a
//! single [`AggregatedHookResult`] via
//! [`AggregatedHookResult::merge`].
//!
//! Integration with the orchestrator (`EventHandle<T>` impls, per-event
//! side effects, tool input overrides, etc.) is wired through the
//! [`PluginHookHandler::dispatch`] method, which the orchestrator
//! bolts onto each lifecycle event's existing call site.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use forge_domain::{
    AgentHookCommand, AggregatedHookResult, ConfigChangePayload, Conversation, CwdChangedPayload,
    ElicitationPayload, ElicitationResultPayload, EventData, EventHandle, FileChangedPayload,
    HookCommand, HookEventName, HookInput, HookInputBase, HookInputPayload, HookOutcome,
    HttpHookCommand, InstructionsLoadedPayload, NotificationPayload, PermissionDeniedPayload,
    PermissionRequestPayload, PostCompactPayload, PostToolUseFailurePayload, PostToolUsePayload,
    PreCompactPayload, PreToolUsePayload, PromptHookCommand, SessionEndPayload,
    SessionStartPayload, SetupPayload, ShellHookCommand, StopFailurePayload, StopPayload,
    SubagentStartPayload, SubagentStopPayload, UserPromptSubmitPayload, WorktreeCreatePayload,
    WorktreeRemovePayload,
};
use tokio::sync::Mutex;

use crate::SessionEnvCache;
use crate::hook_matcher::{matches_condition, matches_pattern};
use crate::hook_runtime::{HookConfigLoaderService, HookMatcherWithSource};
use crate::hooks::session_hooks::SessionHookStore;
use crate::infra::HookExecutorInfra;
use crate::services::Services;

// ---- Environment variable names injected into hook subprocesses ----

const FORGE_PROJECT_DIR: &str = "FORGE_PROJECT_DIR";
const FORGE_SESSION_ID: &str = "FORGE_SESSION_ID";
const FORGE_PLUGIN_ROOT: &str = "FORGE_PLUGIN_ROOT";
const FORGE_PLUGIN_DATA: &str = "FORGE_PLUGIN_DATA";
const FORGE_PLUGIN_OPTION_PREFIX: &str = "FORGE_PLUGIN_OPTION_";
const FORGE_ENV_FILE: &str = "FORGE_ENV_FILE";
/// Subdirectory under `base_path` where per-plugin data is stored.
const PLUGIN_DATA_DIR: &str = "plugin-data";

/// Identifier for a single hook command within the merged config. Used
/// to enforce `once` semantics: the first invocation adds the id to
/// `once_fired`; subsequent invocations skip the hook entirely.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct HookId {
    event: HookEventName,
    /// Index of the [`HookMatcherWithSource`] within the per-event list.
    matcher_index: usize,
    /// Index of the [`HookCommand`] within the matcher's `hooks` vec.
    hook_index: usize,
    /// Identifies the source so user/project/plugin hooks with the same
    /// indices don't alias. Uses a short string tag instead of the
    /// `HookConfigSource` enum so `HookId` stays cheap to hash/compare.
    source: String,
}

/// Generic dispatcher over any [`Services`] implementation.
///
/// Cheap to clone — the heavy state (config loader, executor, once-fired
/// set) lives behind `Arc`s.
pub struct PluginHookHandler<S> {
    services: Arc<S>,
    /// Tracks hook ids that have already fired for `once: true` hooks.
    /// Scoped to the handler instance, which in practice is created
    /// per-session/per-conversation.
    once_fired: Arc<Mutex<HashSet<HookId>>>,
    /// Session-scoped cache of environment variables harvested from hook
    /// env files written via `FORGE_ENV_FILE`. Shared with the shell
    /// service so subsequent `BashTool` / `Shell` invocations inherit
    /// these variables.
    session_env_cache: SessionEnvCache,
    /// Session-scoped hook store for dynamic runtime hook registration.
    /// Hooks added here are concatenated with static hooks during dispatch.
    session_hooks: SessionHookStore,
}

impl<S> Clone for PluginHookHandler<S> {
    fn clone(&self) -> Self {
        Self {
            services: Arc::clone(&self.services),
            once_fired: Arc::clone(&self.once_fired),
            session_env_cache: self.session_env_cache.clone(),
            session_hooks: self.session_hooks.clone(),
        }
    }
}

impl<S: Services> PluginHookHandler<S> {
    /// Create a new dispatcher backed by the given [`Services`] handle.
    pub fn new(services: Arc<S>) -> Self {
        Self::with_env_cache(services, SessionEnvCache::new())
    }

    /// Create a new dispatcher with a shared [`SessionHookStore`].
    ///
    /// Not used in production yet — the default constructor creates its
    /// own empty store. This entry point exists for future dynamic
    /// per-session hook registration.
    #[allow(dead_code)] // Extension point: dynamic per-session hook registration.
    pub fn with_session_hooks(services: Arc<S>, session_hooks: SessionHookStore) -> Self {
        Self {
            services,
            once_fired: Arc::new(Mutex::new(HashSet::new())),
            session_env_cache: SessionEnvCache::new(),
            session_hooks,
        }
    }

    /// Create a new dispatcher that shares the given [`SessionEnvCache`]
    /// with external consumers (e.g. the shell service). This lets
    /// environment variables written by hooks via `FORGE_ENV_FILE`
    /// propagate to subsequent shell commands.
    pub fn with_env_cache(services: Arc<S>, session_env_cache: SessionEnvCache) -> Self {
        Self {
            services,
            once_fired: Arc::new(Mutex::new(HashSet::new())),
            session_env_cache,
            session_hooks: SessionHookStore::new(),
        }
    }

    /// Returns a reference to the session environment cache.
    ///
    /// Callers (e.g. the service-layer wiring) can clone this handle and
    /// pass it to the shell service so that variables written by hooks
    /// via `FORGE_ENV_FILE` are visible in subsequent shell commands.
    #[allow(dead_code)] // Extension point: env cache sharing with external consumers.
    pub fn session_env_cache(&self) -> &SessionEnvCache {
        &self.session_env_cache
    }

    /// Returns a reference to the session hook store.
    #[allow(dead_code)] // Extension point: dynamic per-session hook registration.
    pub fn session_hook_store(&self) -> &SessionHookStore {
        &self.session_hooks
    }

    /// Dispatch a single lifecycle event, running every matching hook in
    /// parallel and returning the aggregated result.
    ///
    /// # Arguments
    ///
    /// - `event` — the lifecycle event being fired.
    /// - `tool_name` — the tool name associated with the event, used for
    ///   matcher evaluation. `None` for events without a tool scope (e.g.
    ///   `SessionStart`), which is equivalent to an empty string (any matcher
    ///   that isn't an exact-string match still fires).
    /// - `tool_input` — the tool input JSON, used by the `if` condition
    ///   matcher. `None` for events without tool input.
    /// - `input` — the fully-populated [`HookInput`] written to each hook's
    ///   stdin / posted as the HTTP body.
    ///
    /// # Errors
    ///
    /// Returns an error only if the config loader fails. Per-hook
    /// execution errors are folded into the returned
    /// [`AggregatedHookResult`] as `NonBlockingError` entries.
    pub async fn dispatch(
        &self,
        event: HookEventName,
        tool_name: Option<&str>,
        tool_input: Option<&serde_json::Value>,
        input: HookInput,
    ) -> anyhow::Result<AggregatedHookResult> {
        let merged = self.services.hook_config_loader().load().await?;

        let static_matchers = merged
            .entries
            .get(&event)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);

        // Load session hooks for this session and event, then combine
        // with static matchers so both sources flow through the same
        // match/filter/once pipeline.
        let session_id = &input.base.session_id;
        let session_matchers = self.session_hooks.get_hooks(session_id, &event).await;

        if static_matchers.is_empty() && session_matchers.is_empty() {
            return Ok(AggregatedHookResult::default());
        }

        // Concatenate static + session matchers into a single owned vec.
        let all_matchers: Vec<HookMatcherWithSource> = static_matchers
            .iter()
            .cloned()
            .chain(session_matchers)
            .collect();

        // Collect every hook that passes matcher + condition + once
        // filters. Uses the shared `collect_pending_hooks` function so
        // unit tests exercise the exact same match/filter/once logic.
        let pending = collect_pending_hooks(
            &all_matchers,
            &event,
            tool_name,
            tool_input,
            &self.once_fired,
        )
        .await;

        if pending.is_empty() {
            return Ok(AggregatedHookResult::default());
        }

        // Second pass: run every surviving hook in parallel. Each future
        // returns a `HookExecResult` (or an error which we translate into
        // a NonBlockingError so the aggregator still sees a record).
        //
        // We split the once-ids out so we can pair them with results
        // after execution to decide whether to mark a once-hook as fired.
        let executor = self.services.hook_executor();
        let base_path = self.services.get_environment().base_path;
        let (once_ids, cmd_src_pairs): (
            Vec<Option<HookId>>,
            Vec<(HookCommand, HookMatcherWithSource)>,
        ) = pending
            .into_iter()
            .map(|(cmd, src, id)| (id, (cmd, src)))
            .unzip();

        // Each future returns (Result<HookExecResult>, Option<PathBuf>)
        // where the PathBuf is the FORGE_ENV_FILE path when one was set.
        let dispatched_event = event.clone();
        let futures = cmd_src_pairs.into_iter().map(|(cmd, source)| {
            let input = input.clone();
            let base_path = base_path.clone();
            let dispatched_event = dispatched_event.clone();
            async move {
                match cmd {
                    HookCommand::Command(ref shell) => {
                        // Validate plugin root exists (if this hook comes from a plugin)
                        if let Some(ref root) = source.plugin_root
                            && !root.exists()
                        {
                            tracing::warn!(
                                plugin_root = %root.display(),
                                "plugin directory no longer exists; skipping hook"
                            );
                            return (
                                Err(anyhow::anyhow!(
                                    "plugin directory does not exist: {}",
                                    root.display()
                                )),
                                None,
                            );
                        }

                        // Build FORGE_* env vars from the available context.
                        let mut env_vars = HashMap::new();
                        env_vars.insert(
                            FORGE_PROJECT_DIR.to_string(),
                            input.base.cwd.display().to_string(),
                        );
                        env_vars
                            .insert(FORGE_SESSION_ID.to_string(), input.base.session_id.clone());
                        if let Some(ref root) = source.plugin_root {
                            env_vars
                                .insert(FORGE_PLUGIN_ROOT.to_string(), root.display().to_string());
                        }
                        if let Some(ref name) = source.plugin_name {
                            let data_dir = base_path.join(PLUGIN_DATA_DIR).join(name);
                            env_vars.insert(
                                FORGE_PLUGIN_DATA.to_string(),
                                data_dir.display().to_string(),
                            );
                        }

                        // Inject FORGE_PLUGIN_OPTION_* from user-configured plugin options.
                        for (key, val) in &source.plugin_options {
                            let env_key = format!(
                                "{}{}",
                                FORGE_PLUGIN_OPTION_PREFIX,
                                key.to_uppercase().replace('-', "_")
                            );
                            env_vars.insert(env_key, val.clone());
                        }

                        // Set FORGE_ENV_FILE for events that support env-file
                        // write-back.
                        let env_file_path = if dispatched_event.supports_env_file() {
                            let env_file = std::env::temp_dir().join(format!(
                                "forge-hook-env-{}-{}",
                                input.base.session_id,
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_nanos()
                            ));
                            env_vars
                                .insert(FORGE_ENV_FILE.to_string(), env_file.display().to_string());
                            Some(env_file)
                        } else {
                            None
                        };

                        (
                            executor.execute_shell(shell, &input, env_vars).await,
                            env_file_path,
                        )
                    }
                    HookCommand::Http(ref http) => {
                        (executor.execute_http(http, &input).await, None)
                    }
                    HookCommand::Prompt(ref prompt) => {
                        (executor.execute_prompt(prompt, &input).await, None)
                    }
                    HookCommand::Agent(ref agent) => {
                        (executor.execute_agent(agent, &input).await, None)
                    }
                }
            }
        });

        let results = futures::future::join_all(futures).await;

        // Mark once-hooks as fired only on success, so failed hooks
        // can retry on the next event dispatch.
        let mut once_fired = self.once_fired.lock().await;
        let mut aggregated = AggregatedHookResult::default();
        let mut env_file_paths: Vec<PathBuf> = Vec::new();
        for (once_id, (result, env_file_path)) in once_ids.into_iter().zip(results) {
            match result {
                Ok(ref exec) if exec.outcome == HookOutcome::Success => {
                    if let Some(id) = once_id {
                        once_fired.insert(id);
                    }
                    // Collect env file paths from successful hooks for
                    // read-back after aggregation.
                    if let Some(path) = env_file_path {
                        env_file_paths.push(path);
                    }
                    aggregated.merge(exec.clone());
                }
                Ok(exec) => aggregated.merge(exec),
                Err(e) => {
                    // Per-hook infrastructure error — log and continue so a
                    // single crashed executor never blocks a lifecycle
                    // event.
                    tracing::warn!(
                        error = %e,
                        "hook executor returned an error; skipping this hook"
                    );
                }
            }
        }

        // Read back FORGE_ENV_FILE contents from successful hooks and
        // merge their KEY=VALUE pairs into the session env cache.
        for env_path in &env_file_paths {
            if let Err(e) = self.session_env_cache.ingest_env_file(env_path).await {
                tracing::warn!(
                    path = %env_path.display(),
                    error = %e,
                    "failed to ingest hook env file; variables from this hook will be missing"
                );
            }
            // Best-effort cleanup of the temp file.
            let _ = tokio::fs::remove_file(env_path).await;
        }

        Ok(aggregated)
    }
}

/// Returns the optional `if` condition for any hook variant.
fn condition_for(cmd: &HookCommand) -> Option<&str> {
    match cmd {
        HookCommand::Command(ShellHookCommand { condition, .. })
        | HookCommand::Prompt(PromptHookCommand { condition, .. })
        | HookCommand::Http(HttpHookCommand { condition, .. })
        | HookCommand::Agent(AgentHookCommand { condition, .. }) => condition.as_deref(),
    }
}

/// Returns `true` if the hook declares `once: true`.
fn is_once(cmd: &HookCommand) -> bool {
    match cmd {
        HookCommand::Command(shell) => shell.once,
        HookCommand::Prompt(prompt) => prompt.once,
        HookCommand::Http(http) => http.once,
        HookCommand::Agent(agent) => agent.once,
    }
}

/// Short string tag used as part of [`HookId`] so per-source hooks with
/// matching indices never collide in the `once_fired` set.
fn source_tag(src: &HookMatcherWithSource) -> String {
    use crate::hook_runtime::HookConfigSource;
    match src.source {
        HookConfigSource::UserGlobal => "user".to_string(),
        HookConfigSource::Project => "project".to_string(),
        HookConfigSource::Plugin => {
            format!("plugin:{}", src.plugin_name.as_deref().unwrap_or(""))
        }
        HookConfigSource::Managed => "managed".to_string(),
        HookConfigSource::Session => "session".to_string(),
    }
}

/// Collect hooks that should execute for the given event, applying
/// matcher patterns, condition checks, and once-firing deduplication.
///
/// Used by both production [`PluginHookHandler::dispatch`] and the
/// test `ExplicitDispatcher` so the match/filter/once logic is shared
/// in a single source of truth. Changes here are automatically
/// exercised by both production and unit-test code paths.
///
/// Returns a vector of `(HookCommand, HookMatcherWithSource,
/// Option<HookId>)` tuples for hooks that passed all filters, plus a
/// set of `HookId`s that were claimed by `once: true` hooks in this
/// batch (used internally; callers typically ignore this).
async fn collect_pending_hooks(
    matchers: &[HookMatcherWithSource],
    event: &HookEventName,
    tool_name: Option<&str>,
    tool_input: Option<&serde_json::Value>,
    once_fired: &Mutex<HashSet<HookId>>,
) -> Vec<(HookCommand, HookMatcherWithSource, Option<HookId>)> {
    let empty = serde_json::Value::Null;
    let tn = tool_name.unwrap_or("");
    let ti = tool_input.unwrap_or(&empty);

    let mut pending: Vec<(HookCommand, HookMatcherWithSource, Option<HookId>)> = Vec::new();
    let mut once_claimed: HashSet<HookId> = HashSet::new();
    {
        let fired = once_fired.lock().await;
        for (mi, matcher_with_source) in matchers.iter().enumerate() {
            let pat = matcher_with_source.matcher.matcher.as_deref().unwrap_or("");
            if !matches_pattern(pat, tn) {
                continue;
            }
            for (hi, cmd) in matcher_with_source.matcher.hooks.iter().enumerate() {
                if let Some(c) = condition_for(cmd)
                    && !matches_condition(c, tn, ti)
                {
                    continue;
                }
                if is_once(cmd) {
                    let id = HookId {
                        event: event.clone(),
                        matcher_index: mi,
                        hook_index: hi,
                        source: source_tag(matcher_with_source),
                    };
                    if fired.contains(&id) || once_claimed.contains(&id) {
                        continue;
                    }
                    once_claimed.insert(id.clone());
                    pending.push((cmd.clone(), matcher_with_source.clone(), Some(id)));
                } else {
                    pending.push((cmd.clone(), matcher_with_source.clone(), None));
                }
            }
        }
    }
    pending
}

/// Execute a list of pending hooks sequentially, merge each result
/// into an [`AggregatedHookResult`], and mark successful once-hooks as
/// fired.
///
/// `execute_fn` is called for each hook and returns a
/// [`HookExecResult`]. In tests this returns canned results; production
/// code uses its own parallel execution path via `futures::future::join_all`.
#[cfg(test)]
async fn execute_and_merge<F, Fut>(
    pending: Vec<(HookCommand, HookMatcherWithSource, Option<HookId>)>,
    once_fired: &Mutex<HashSet<HookId>>,
    mut execute_fn: F,
) -> AggregatedHookResult
where
    F: FnMut(&HookCommand, &HookMatcherWithSource) -> Fut,
    Fut: std::future::Future<Output = forge_domain::HookExecResult>,
{
    let mut fired_lock = once_fired.lock().await;
    let mut aggregated = AggregatedHookResult::default();
    for (cmd, src, once_id) in pending {
        let exec = execute_fn(&cmd, &src).await;
        if exec.outcome == HookOutcome::Success
            && let Some(id) = once_id
        {
            fired_lock.insert(id);
        }
        aggregated.merge(exec);
    }
    aggregated
}

// ---- EventHandle impls for the plugin-hook lifecycle events ----
//
// Each impl maps an [`EventData<...Payload>`] into a [`HookInput`] via
// [`build_hook_input`], then forwards to
// [`PluginHookHandler::dispatch`]. The resulting
// [`AggregatedHookResult`] is written into `conversation.hook_result`
// so downstream orchestrator code can consume it.
//
// The trait implementations do NOT fire these events themselves — they
// only define *how* the handler reacts if the orchestrator dispatches
// the matching [`crate::forge_domain::LifecycleEvent`] variant. The
// orchestrator wires the fire sites.

/// Build a [`HookInput`] from any [`EventData`] payload whose Rust type
/// converts into [`HookInputPayload`]. Centralises the base-field copy
/// (session_id, transcript_path, ...) so the individual trait impls
/// remain one-liners.
///
/// **`agent_id` / `agent_type` semantics (matching Claude Code):**
/// - `agent_type` is always derived from `event.agent.id` — a semantic name
///   like `"forge"`, `"code-reviewer"`, etc.
/// - `agent_id` is `None` for the main REPL thread (most events). For sub-agent
///   events (`SubagentStart`, `SubagentStop`), the caller passes `Some(id)` via
///   the `subagent_id` parameter, where `id` comes from the payload's
///   `agent_id` field.
fn build_hook_input<P>(
    event: &EventData<P>,
    hook_event_name: &HookEventName,
    payload: HookInputPayload,
    subagent_id: Option<String>,
) -> HookInput
where
    P: Send + Sync,
{
    let agent_type = event.agent.id.as_str().to_string();
    HookInput {
        base: HookInputBase {
            session_id: event.session_id.clone(),
            transcript_path: event.transcript_path.clone(),
            cwd: event.cwd.clone(),
            permission_mode: event.permission_mode.clone(),
            agent_id: subagent_id,
            agent_type: Some(agent_type),
            hook_event_name: hook_event_name.as_str().to_string(),
        },
        payload,
    }
}

#[async_trait]
impl<S: Services> EventHandle<EventData<PreToolUsePayload>> for PluginHookHandler<S> {
    async fn handle(
        &self,
        event: &EventData<PreToolUsePayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let input = build_hook_input(
            event,
            &HookEventName::PreToolUse,
            HookInputPayload::PreToolUse {
                tool_name: event.payload.tool_name.clone(),
                tool_input: event.payload.tool_input.clone(),
                tool_use_id: event.payload.tool_use_id.clone(),
            },
            None,
        );
        let aggregated = self
            .dispatch(
                HookEventName::PreToolUse,
                Some(&event.payload.tool_name),
                Some(&event.payload.tool_input),
                input,
            )
            .await?;
        conversation.hook_result = aggregated;
        Ok(())
    }
}

#[async_trait]
impl<S: Services> EventHandle<EventData<PostToolUsePayload>> for PluginHookHandler<S> {
    async fn handle(
        &self,
        event: &EventData<PostToolUsePayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let input = build_hook_input(
            event,
            &HookEventName::PostToolUse,
            HookInputPayload::PostToolUse {
                tool_name: event.payload.tool_name.clone(),
                tool_input: event.payload.tool_input.clone(),
                tool_response: event.payload.tool_response.clone(),
                tool_use_id: event.payload.tool_use_id.clone(),
            },
            None,
        );
        let aggregated = self
            .dispatch(
                HookEventName::PostToolUse,
                Some(&event.payload.tool_name),
                Some(&event.payload.tool_input),
                input,
            )
            .await?;
        conversation.hook_result = aggregated;
        Ok(())
    }
}

#[async_trait]
impl<S: Services> EventHandle<EventData<PostToolUseFailurePayload>> for PluginHookHandler<S> {
    async fn handle(
        &self,
        event: &EventData<PostToolUseFailurePayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let input = build_hook_input(
            event,
            &HookEventName::PostToolUseFailure,
            HookInputPayload::PostToolUseFailure {
                tool_name: event.payload.tool_name.clone(),
                tool_input: event.payload.tool_input.clone(),
                tool_use_id: event.payload.tool_use_id.clone(),
                error: event.payload.error.clone(),
                is_interrupt: event.payload.is_interrupt,
            },
            None,
        );
        let aggregated = self
            .dispatch(
                HookEventName::PostToolUseFailure,
                Some(&event.payload.tool_name),
                Some(&event.payload.tool_input),
                input,
            )
            .await?;
        conversation.hook_result = aggregated;
        Ok(())
    }
}

#[async_trait]
impl<S: Services> EventHandle<EventData<UserPromptSubmitPayload>> for PluginHookHandler<S> {
    async fn handle(
        &self,
        event: &EventData<UserPromptSubmitPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let input = build_hook_input(
            event,
            &HookEventName::UserPromptSubmit,
            HookInputPayload::UserPromptSubmit { prompt: event.payload.prompt.clone() },
            None,
        );
        let aggregated = self
            .dispatch(HookEventName::UserPromptSubmit, None, None, input)
            .await?;
        conversation.hook_result = aggregated;
        Ok(())
    }
}

#[async_trait]
impl<S: Services> EventHandle<EventData<SessionStartPayload>> for PluginHookHandler<S> {
    async fn handle(
        &self,
        event: &EventData<SessionStartPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let input = build_hook_input(
            event,
            &HookEventName::SessionStart,
            HookInputPayload::SessionStart {
                source: event.payload.source.as_wire_str().to_string(),
                model: event.payload.model.clone(),
            },
            None,
        );
        let aggregated = self
            .dispatch(
                HookEventName::SessionStart,
                Some(event.payload.source.as_wire_str()),
                None,
                input,
            )
            .await?;
        conversation.hook_result = aggregated;
        Ok(())
    }
}

#[async_trait]
impl<S: Services> EventHandle<EventData<SessionEndPayload>> for PluginHookHandler<S> {
    async fn handle(
        &self,
        event: &EventData<SessionEndPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let input = build_hook_input(
            event,
            &HookEventName::SessionEnd,
            HookInputPayload::SessionEnd { reason: event.payload.reason.as_wire_str().to_string() },
            None,
        );
        let aggregated = self
            .dispatch(
                HookEventName::SessionEnd,
                Some(event.payload.reason.as_wire_str()),
                None,
                input,
            )
            .await?;
        conversation.hook_result = aggregated;
        Ok(())
    }
}

#[async_trait]
impl<S: Services> EventHandle<EventData<StopPayload>> for PluginHookHandler<S> {
    async fn handle(
        &self,
        event: &EventData<StopPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let input = build_hook_input(
            event,
            &HookEventName::Stop,
            HookInputPayload::Stop {
                stop_hook_active: event.payload.stop_hook_active,
                last_assistant_message: event.payload.last_assistant_message.clone(),
            },
            None,
        );
        let aggregated = self
            .dispatch(HookEventName::Stop, None, None, input)
            .await?;
        conversation.hook_result = aggregated;
        Ok(())
    }
}

#[async_trait]
impl<S: Services> EventHandle<EventData<StopFailurePayload>> for PluginHookHandler<S> {
    async fn handle(
        &self,
        event: &EventData<StopFailurePayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let input = build_hook_input(
            event,
            &HookEventName::StopFailure,
            HookInputPayload::StopFailure {
                error: event.payload.error.clone(),
                error_details: event.payload.error_details.clone(),
                last_assistant_message: event.payload.last_assistant_message.clone(),
            },
            None,
        );
        let aggregated = self
            .dispatch(
                HookEventName::StopFailure,
                Some(&event.payload.error),
                None,
                input,
            )
            .await?;
        conversation.hook_result = aggregated;

        // Clean up session-scoped hooks to prevent unbounded memory
        // growth. Called after dispatch so all SessionEnd hooks have
        // finished before their entries are removed.
        self.session_hooks.clear_session(&event.session_id).await;

        Ok(())
    }
}

#[async_trait]
impl<S: Services> EventHandle<EventData<PreCompactPayload>> for PluginHookHandler<S> {
    async fn handle(
        &self,
        event: &EventData<PreCompactPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let input = build_hook_input(
            event,
            &HookEventName::PreCompact,
            HookInputPayload::PreCompact {
                trigger: event.payload.trigger.as_wire_str().to_string(),
                custom_instructions: event.payload.custom_instructions.clone(),
            },
            None,
        );
        let aggregated = self
            .dispatch(
                HookEventName::PreCompact,
                Some(event.payload.trigger.as_wire_str()),
                None,
                input,
            )
            .await?;
        conversation.hook_result = aggregated;
        Ok(())
    }
}

#[async_trait]
impl<S: Services> EventHandle<EventData<PostCompactPayload>> for PluginHookHandler<S> {
    async fn handle(
        &self,
        event: &EventData<PostCompactPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let input = build_hook_input(
            event,
            &HookEventName::PostCompact,
            HookInputPayload::PostCompact {
                trigger: event.payload.trigger.as_wire_str().to_string(),
                compact_summary: event.payload.compact_summary.clone(),
            },
            None,
        );
        let aggregated = self
            .dispatch(
                HookEventName::PostCompact,
                Some(event.payload.trigger.as_wire_str()),
                None,
                input,
            )
            .await?;
        conversation.hook_result = aggregated;
        Ok(())
    }
}

// ---- Notification / Setup / Config events ----

#[async_trait]
impl<S: Services> EventHandle<EventData<NotificationPayload>> for PluginHookHandler<S> {
    async fn handle(
        &self,
        event: &EventData<NotificationPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let input = build_hook_input(
            event,
            &HookEventName::Notification,
            HookInputPayload::Notification {
                message: event.payload.message.clone(),
                title: event.payload.title.clone(),
                notification_type: event.payload.notification_type.clone(),
            },
            None,
        );
        // Notification matchers filter on the `notification_type` field
        // (e.g. `"idle_prompt"`, `"auth_success"`) via the standard
        // matcher pipeline. Tool-input condition matching is not
        // applicable here.
        let aggregated = self
            .dispatch(
                HookEventName::Notification,
                Some(&event.payload.notification_type),
                None,
                input,
            )
            .await?;
        conversation.hook_result = aggregated;
        Ok(())
    }
}

#[async_trait]
impl<S: Services> EventHandle<EventData<SetupPayload>> for PluginHookHandler<S> {
    async fn handle(
        &self,
        event: &EventData<SetupPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let trigger_wire = event.payload.trigger.as_wire_str();
        let input = build_hook_input(
            event,
            &HookEventName::Setup,
            HookInputPayload::Setup { trigger: trigger_wire.to_string() },
            None,
        );
        // Setup matchers filter on the trigger string (`"init"` /
        // `"maintenance"`).
        let aggregated = self
            .dispatch(HookEventName::Setup, Some(trigger_wire), None, input)
            .await?;
        conversation.hook_result = aggregated;
        Ok(())
    }
}

#[async_trait]
impl<S: Services> EventHandle<EventData<ConfigChangePayload>> for PluginHookHandler<S> {
    async fn handle(
        &self,
        event: &EventData<ConfigChangePayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let source_wire = event.payload.source.as_wire_str();
        let input = build_hook_input(
            event,
            &HookEventName::ConfigChange,
            HookInputPayload::ConfigChange {
                source: source_wire.to_string(),
                file_path: event.payload.file_path.clone(),
            },
            None,
        );
        // ConfigChange matchers filter on the source wire string
        // (`"user_settings"`, `"plugins"`, ...).
        let aggregated = self
            .dispatch(HookEventName::ConfigChange, Some(source_wire), None, input)
            .await?;
        conversation.hook_result = aggregated;
        Ok(())
    }
}

// ---- Subagent / Permission / File / Worktree events ----

#[async_trait]
impl<S: Services> EventHandle<EventData<SubagentStartPayload>> for PluginHookHandler<S> {
    async fn handle(
        &self,
        event: &EventData<SubagentStartPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let input = build_hook_input(
            event,
            &HookEventName::SubagentStart,
            HookInputPayload::SubagentStart {
                agent_id: event.payload.agent_id.clone(),
                agent_type: event.payload.agent_type.clone(),
            },
            Some(event.payload.agent_id.clone()),
        );
        // SubagentStart matchers filter on the `agent_type` field
        // (e.g. `"code-reviewer"`, `"muse"`).
        let aggregated = self
            .dispatch(
                HookEventName::SubagentStart,
                Some(&event.payload.agent_type),
                None,
                input,
            )
            .await?;
        conversation.hook_result = aggregated;
        Ok(())
    }
}

#[async_trait]
impl<S: Services> EventHandle<EventData<SubagentStopPayload>> for PluginHookHandler<S> {
    async fn handle(
        &self,
        event: &EventData<SubagentStopPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let input = build_hook_input(
            event,
            &HookEventName::SubagentStop,
            HookInputPayload::SubagentStop {
                agent_id: event.payload.agent_id.clone(),
                agent_type: event.payload.agent_type.clone(),
                agent_transcript_path: event.payload.agent_transcript_path.clone(),
                stop_hook_active: event.payload.stop_hook_active,
                last_assistant_message: event.payload.last_assistant_message.clone(),
            },
            Some(event.payload.agent_id.clone()),
        );
        // SubagentStop matchers filter on the `agent_type` field.
        let aggregated = self
            .dispatch(
                HookEventName::SubagentStop,
                Some(&event.payload.agent_type),
                None,
                input,
            )
            .await?;
        conversation.hook_result = aggregated;
        Ok(())
    }
}

#[async_trait]
impl<S: Services> EventHandle<EventData<PermissionRequestPayload>> for PluginHookHandler<S> {
    async fn handle(
        &self,
        event: &EventData<PermissionRequestPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let input = build_hook_input(
            event,
            &HookEventName::PermissionRequest,
            HookInputPayload::PermissionRequest {
                tool_name: event.payload.tool_name.clone(),
                tool_input: event.payload.tool_input.clone(),
                permission_suggestions: event.payload.permission_suggestions.clone(),
            },
            None,
        );
        // PermissionRequest matchers filter on the tool name, mirroring
        // PreToolUse semantics.
        let aggregated = self
            .dispatch(
                HookEventName::PermissionRequest,
                Some(&event.payload.tool_name),
                Some(&event.payload.tool_input),
                input,
            )
            .await?;
        conversation.hook_result = aggregated;
        Ok(())
    }
}

#[async_trait]
impl<S: Services> EventHandle<EventData<PermissionDeniedPayload>> for PluginHookHandler<S> {
    async fn handle(
        &self,
        event: &EventData<PermissionDeniedPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let input = build_hook_input(
            event,
            &HookEventName::PermissionDenied,
            HookInputPayload::PermissionDenied {
                tool_name: event.payload.tool_name.clone(),
                tool_input: event.payload.tool_input.clone(),
                tool_use_id: event.payload.tool_use_id.clone(),
                reason: event.payload.reason.clone(),
            },
            None,
        );
        // PermissionDenied matchers filter on the tool name.
        let mut aggregated = self
            .dispatch(
                HookEventName::PermissionDenied,
                Some(&event.payload.tool_name),
                Some(&event.payload.tool_input),
                input,
            )
            .await?;
        // PermissionDenied is observability-only per Claude Code's contract.
        // Strip permission-sensitive fields so hooks cannot flip a denied
        // decision back to Allow or mutate the tool input.
        aggregated.permission_behavior = None;
        aggregated.updated_input = None;
        aggregated.updated_permissions = None;
        aggregated.interrupt = false;
        aggregated.retry = false;
        conversation.hook_result = aggregated;
        Ok(())
    }
}

#[async_trait]
impl<S: Services> EventHandle<EventData<CwdChangedPayload>> for PluginHookHandler<S> {
    async fn handle(
        &self,
        event: &EventData<CwdChangedPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let input = build_hook_input(
            event,
            &HookEventName::CwdChanged,
            HookInputPayload::CwdChanged {
                old_cwd: event.payload.old_cwd.clone(),
                new_cwd: event.payload.new_cwd.clone(),
            },
            None,
        );
        // CwdChanged broadcasts — no matcher; dispatch with `None`.
        let aggregated = self
            .dispatch(HookEventName::CwdChanged, None, None, input)
            .await?;
        conversation.hook_result = aggregated;
        Ok(())
    }
}

#[async_trait]
impl<S: Services> EventHandle<EventData<FileChangedPayload>> for PluginHookHandler<S> {
    async fn handle(
        &self,
        event: &EventData<FileChangedPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let file_name = event
            .payload
            .file_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| event.payload.file_path.to_string_lossy().into_owned());
        let input = build_hook_input(
            event,
            &HookEventName::FileChanged,
            HookInputPayload::FileChanged {
                file_path: event.payload.file_path.clone(),
                event: event.payload.event.as_wire_str().to_string(),
            },
            None,
        );
        // FileChanged matchers filter on the file basename.
        let aggregated = self
            .dispatch(HookEventName::FileChanged, Some(&file_name), None, input)
            .await?;
        conversation.hook_result = aggregated;
        Ok(())
    }
}

#[async_trait]
impl<S: Services> EventHandle<EventData<WorktreeCreatePayload>> for PluginHookHandler<S> {
    async fn handle(
        &self,
        event: &EventData<WorktreeCreatePayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let name = event.payload.name.clone();
        let input = build_hook_input(
            event,
            &HookEventName::WorktreeCreate,
            HookInputPayload::WorktreeCreate { name: name.clone() },
            None,
        );
        // Claude Code does not set a matchQuery for WorktreeCreate — all
        // registered matchers fire unconditionally.
        let aggregated = self
            .dispatch(HookEventName::WorktreeCreate, None, None, input)
            .await?;
        conversation.hook_result = aggregated;
        Ok(())
    }
}

#[async_trait]
impl<S: Services> EventHandle<EventData<WorktreeRemovePayload>> for PluginHookHandler<S> {
    async fn handle(
        &self,
        event: &EventData<WorktreeRemovePayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let input = build_hook_input(
            event,
            &HookEventName::WorktreeRemove,
            HookInputPayload::WorktreeRemove { worktree_path: event.payload.worktree_path.clone() },
            None,
        );
        // Claude Code does not set a matchQuery for WorktreeRemove — all
        // registered matchers fire unconditionally.
        let aggregated = self
            .dispatch(HookEventName::WorktreeRemove, None, None, input)
            .await?;
        conversation.hook_result = aggregated;
        Ok(())
    }
}

// ---- InstructionsLoaded event ----

#[async_trait]
impl<S: Services> EventHandle<EventData<InstructionsLoadedPayload>> for PluginHookHandler<S> {
    async fn handle(
        &self,
        event: &EventData<InstructionsLoadedPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let reason = event.payload.load_reason.as_wire_str().to_string();
        let input = build_hook_input(
            event,
            &HookEventName::InstructionsLoaded,
            HookInputPayload::InstructionsLoaded {
                file_path: event.payload.file_path.clone(),
                memory_type: event.payload.memory_type.as_wire_str().to_string(),
                load_reason: reason.clone(),
                globs: event.payload.globs.clone(),
                trigger_file_path: event.payload.trigger_file_path.clone(),
                parent_file_path: event.payload.parent_file_path.clone(),
            },
            None,
        );
        // Matcher is the load_reason wire string.
        let aggregated = self
            .dispatch(
                HookEventName::InstructionsLoaded,
                Some(&reason),
                None,
                input,
            )
            .await?;
        conversation.hook_result = aggregated;
        Ok(())
    }
}

// ---- Elicitation events ----

#[async_trait]
impl<S: Services> EventHandle<EventData<ElicitationPayload>> for PluginHookHandler<S> {
    async fn handle(
        &self,
        event: &EventData<ElicitationPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let input = build_hook_input(
            event,
            &HookEventName::Elicitation,
            HookInputPayload::Elicitation {
                server_name: event.payload.server_name.clone(),
                message: event.payload.message.clone(),
                requested_schema: event.payload.requested_schema.clone(),
                mode: event.payload.mode.clone(),
                url: event.payload.url.clone(),
                elicitation_id: event.payload.elicitation_id.clone(),
            },
            None,
        );
        // Elicitation matchers filter on the MCP server name.
        let aggregated = self
            .dispatch(
                HookEventName::Elicitation,
                Some(&event.payload.server_name),
                None,
                input,
            )
            .await?;
        conversation.hook_result = aggregated;
        Ok(())
    }
}

#[async_trait]
impl<S: Services> EventHandle<EventData<ElicitationResultPayload>> for PluginHookHandler<S> {
    async fn handle(
        &self,
        event: &EventData<ElicitationResultPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let input = build_hook_input(
            event,
            &HookEventName::ElicitationResult,
            HookInputPayload::ElicitationResult {
                server_name: event.payload.server_name.clone(),
                action: event.payload.action.clone(),
                content: event.payload.content.clone(),
                mode: event.payload.mode.clone(),
                elicitation_id: event.payload.elicitation_id.clone(),
            },
            None,
        );
        // ElicitationResult matchers filter on the MCP server name.
        let aggregated = self
            .dispatch(
                HookEventName::ElicitationResult,
                Some(&event.payload.server_name),
                None,
                input,
            )
            .await?;
        conversation.hook_result = aggregated;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use forge_domain::{
        HookEventName, HookExecResult, HookInput, HookInputBase, HookInputPayload, HookOutcome,
        HookOutput, HookSpecificOutput, PermissionBehavior, PermissionDecision, SyncHookOutput,
    };
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;
    use crate::hook_runtime::{HookMatcherWithSource, MergedHooksConfig};

    fn sample_input(event_name: &str) -> HookInput {
        HookInput {
            base: HookInputBase {
                session_id: "sess".to_string(),
                transcript_path: PathBuf::from("/tmp/t.json"),
                cwd: PathBuf::from("/tmp"),
                permission_mode: None,
                agent_id: None,
                agent_type: None,
                hook_event_name: event_name.to_string(),
            },
            payload: HookInputPayload::PreToolUse {
                tool_name: "Bash".to_string(),
                tool_input: json!({"command": "echo hi"}),
                tool_use_id: "toolu_1".to_string(),
            },
        }
    }

    /// Test-only dispatcher that exercises the same match/filter/once
    /// logic as the production `PluginHookHandler::dispatch` via the
    /// shared `collect_pending_hooks` and `execute_and_merge` functions.
    /// Uses `StubExecutor::canned_success()` instead of real executor
    /// calls so tests stay fast and deterministic.
    struct ExplicitDispatcher {
        merged: Arc<MergedHooksConfig>,
        executor: StubExecutor,
        once_fired: Arc<Mutex<HashSet<HookId>>>,
    }

    #[derive(Default, Clone)]
    struct StubExecutor {
        calls: Arc<Mutex<Vec<String>>>,
    }

    impl StubExecutor {
        fn canned_success() -> forge_domain::HookExecResult {
            forge_domain::HookExecResult {
                outcome: HookOutcome::Success,
                output: None,
                raw_stdout: "canned".to_string(),
                raw_stderr: String::new(),
                exit_code: Some(0),
            }
        }
    }

    impl ExplicitDispatcher {
        fn new(merged: MergedHooksConfig) -> Self {
            Self {
                merged: Arc::new(merged),
                executor: StubExecutor::default(),
                once_fired: Arc::new(Mutex::new(HashSet::new())),
            }
        }

        async fn dispatch(
            &self,
            event: HookEventName,
            tool_name: Option<&str>,
            tool_input: Option<&serde_json::Value>,
            _input: HookInput,
        ) -> AggregatedHookResult {
            let Some(matchers) = self.merged.entries.get(&event) else {
                return AggregatedHookResult::default();
            };

            let pending =
                collect_pending_hooks(matchers, &event, tool_name, tool_input, &self.once_fired)
                    .await;

            let executor = &self.executor;
            execute_and_merge(pending, &self.once_fired, |_cmd, _src| {
                let executor = executor.clone();
                async move {
                    executor.calls.lock().await.push("hit".to_string());
                    StubExecutor::canned_success()
                }
            })
            .await
        }

        /// Mirror of [`Self::dispatch`] that folds pre-canned
        /// [`HookExecResult`]s into the aggregate instead of the default
        /// `canned_success()` stub. Used by PermissionRequest
        /// merge tests that need the executor to return
        /// [`HookSpecificOutput::PermissionRequest`] values so the
        /// aggregator's permission-merge branch actually runs.
        ///
        /// Results are consumed in matcher+hook iteration order. If
        /// `canned` has fewer entries than matched hooks, the extras fall
        /// back to `StubExecutor::canned_success()`.
        async fn dispatch_with_canned_results(
            &self,
            event: HookEventName,
            tool_name: Option<&str>,
            tool_input: Option<&serde_json::Value>,
            _input: HookInput,
            canned: Vec<HookExecResult>,
        ) -> AggregatedHookResult {
            let Some(matchers) = self.merged.entries.get(&event) else {
                return AggregatedHookResult::default();
            };

            let pending =
                collect_pending_hooks(matchers, &event, tool_name, tool_input, &self.once_fired)
                    .await;

            // Wrap the canned results in an Arc<Mutex<_>> so the closure
            // can drain them in order.
            let canned = Arc::new(Mutex::new(canned));
            let executor = &self.executor;
            execute_and_merge(pending, &self.once_fired, |_cmd, _src| {
                let executor = executor.clone();
                let canned = Arc::clone(&canned);
                async move {
                    executor.calls.lock().await.push("hit".to_string());
                    let mut canned_lock = canned.lock().await;
                    if canned_lock.is_empty() {
                        StubExecutor::canned_success()
                    } else {
                        canned_lock.remove(0)
                    }
                }
            })
            .await
        }
    }

    #[tokio::test]
    async fn test_dispatch_empty_config_returns_default() {
        let dispatcher = ExplicitDispatcher::new(MergedHooksConfig::default());
        let result = dispatcher
            .dispatch(
                HookEventName::PreToolUse,
                Some("Bash"),
                Some(&json!({"command": "ls"})),
                sample_input("PreToolUse"),
            )
            .await;

        assert!(result.blocking_error.is_none());
        assert!(result.additional_contexts.is_empty());
        assert!(result.permission_behavior.is_none());
    }

    #[tokio::test]
    async fn test_dispatch_runs_matching_shell_hook_and_aggregates_stdout() {
        use forge_domain::{HookMatcher, ShellHookCommand};
        let mut merged = MergedHooksConfig::default();
        merged.entries.insert(
            HookEventName::PreToolUse,
            vec![HookMatcherWithSource {
                matcher: HookMatcher {
                    matcher: Some("Bash".to_string()),
                    hooks: vec![HookCommand::Command(ShellHookCommand {
                        command: "echo hi".to_string(),
                        condition: None,
                        shell: None,
                        timeout: None,
                        status_message: None,
                        once: false,
                        async_mode: false,
                        async_rewake: false,
                    })],
                },
                source: crate::hook_runtime::HookConfigSource::UserGlobal,
                plugin_root: None,
                plugin_name: None,
                plugin_options: vec![],
            }],
        );

        let dispatcher = ExplicitDispatcher::new(merged);
        let result = dispatcher
            .dispatch(
                HookEventName::PreToolUse,
                Some("Bash"),
                Some(&json!({"command": "echo hi"})),
                sample_input("PreToolUse"),
            )
            .await;

        // The stub executor returns a Success with "canned" stdout, which
        // the aggregator folds into `additional_contexts`.
        assert_eq!(result.additional_contexts, vec!["canned".to_string()]);
        assert!(result.blocking_error.is_none());
    }

    #[tokio::test]
    async fn test_dispatch_skips_non_matching_matcher() {
        use forge_domain::{HookMatcher, ShellHookCommand};
        let mut merged = MergedHooksConfig::default();
        merged.entries.insert(
            HookEventName::PreToolUse,
            vec![HookMatcherWithSource {
                matcher: HookMatcher {
                    matcher: Some("Write".to_string()),
                    hooks: vec![HookCommand::Command(ShellHookCommand {
                        command: "echo hi".to_string(),
                        condition: None,
                        shell: None,
                        timeout: None,
                        status_message: None,
                        once: false,
                        async_mode: false,
                        async_rewake: false,
                    })],
                },
                source: crate::hook_runtime::HookConfigSource::UserGlobal,
                plugin_root: None,
                plugin_name: None,
                plugin_options: vec![],
            }],
        );

        let dispatcher = ExplicitDispatcher::new(merged);
        let result = dispatcher
            .dispatch(
                HookEventName::PreToolUse,
                Some("Bash"),
                Some(&json!({"command": "echo hi"})),
                sample_input("PreToolUse"),
            )
            .await;

        // No hook matched, so no aggregation happened.
        assert!(result.additional_contexts.is_empty());
    }

    #[tokio::test]
    async fn test_dispatch_respects_once_semantics() {
        use forge_domain::{HookMatcher, ShellHookCommand};
        let mut merged = MergedHooksConfig::default();
        merged.entries.insert(
            HookEventName::PreToolUse,
            vec![HookMatcherWithSource {
                matcher: HookMatcher {
                    matcher: Some("Bash".to_string()),
                    hooks: vec![HookCommand::Command(ShellHookCommand {
                        command: "echo hi".to_string(),
                        condition: None,
                        shell: None,
                        timeout: None,
                        status_message: None,
                        once: true,
                        async_mode: false,
                        async_rewake: false,
                    })],
                },
                source: crate::hook_runtime::HookConfigSource::UserGlobal,
                plugin_root: None,
                plugin_name: None,
                plugin_options: vec![],
            }],
        );

        let dispatcher = ExplicitDispatcher::new(merged);
        // First dispatch — hook fires.
        let first = dispatcher
            .dispatch(
                HookEventName::PreToolUse,
                Some("Bash"),
                Some(&json!({"command": "echo hi"})),
                sample_input("PreToolUse"),
            )
            .await;
        assert_eq!(first.additional_contexts, vec!["canned".to_string()]);

        // Second dispatch — hook has already fired, should be skipped.
        let second = dispatcher
            .dispatch(
                HookEventName::PreToolUse,
                Some("Bash"),
                Some(&json!({"command": "echo hi"})),
                sample_input("PreToolUse"),
            )
            .await;
        assert!(second.additional_contexts.is_empty());
    }

    /// A `once: true` hook that FAILS should NOT be marked as fired, so
    /// it can retry on the next event dispatch. Only successful execution
    /// should permanently mark it.
    #[tokio::test]
    async fn test_dispatch_once_hook_retries_after_failure() {
        use forge_domain::{HookMatcher, ShellHookCommand};
        let mut merged = MergedHooksConfig::default();
        merged.entries.insert(
            HookEventName::PreToolUse,
            vec![HookMatcherWithSource {
                matcher: HookMatcher {
                    matcher: Some("Bash".to_string()),
                    hooks: vec![HookCommand::Command(ShellHookCommand {
                        command: "echo once-fail".to_string(),
                        condition: None,
                        shell: None,
                        timeout: None,
                        status_message: None,
                        once: true,
                        async_mode: false,
                        async_rewake: false,
                    })],
                },
                source: crate::hook_runtime::HookConfigSource::UserGlobal,
                plugin_root: None,
                plugin_name: None,
                plugin_options: vec![],
            }],
        );

        let dispatcher = ExplicitDispatcher::new(merged);

        // First dispatch with a FAILED result — the once-hook should
        // execute but NOT be marked as fired.
        let failed_result = HookExecResult {
            outcome: HookOutcome::NonBlockingError,
            output: None,
            raw_stdout: "error output".to_string(),
            raw_stderr: "something went wrong".to_string(),
            exit_code: Some(1),
        };
        let first = dispatcher
            .dispatch_with_canned_results(
                HookEventName::PreToolUse,
                Some("Bash"),
                Some(&json!({"command": "echo hi"})),
                sample_input("PreToolUse"),
                vec![failed_result],
            )
            .await;
        // The hook ran (non-blocking error is still merged).
        let calls_after_first = dispatcher.executor.calls.lock().await.len();
        assert_eq!(calls_after_first, 1);
        // NonBlockingError stdout is NOT folded into additional_contexts
        // (only Success outcomes contribute context), so contexts remain empty.
        assert!(first.additional_contexts.is_empty());

        // Second dispatch — since the first failed, the once-hook should
        // fire again (retry).
        let second = dispatcher
            .dispatch_with_canned_results(
                HookEventName::PreToolUse,
                Some("Bash"),
                Some(&json!({"command": "echo hi"})),
                sample_input("PreToolUse"),
                vec![StubExecutor::canned_success()],
            )
            .await;
        let calls_after_second = dispatcher.executor.calls.lock().await.len();
        assert_eq!(calls_after_second, 2);
        assert_eq!(second.additional_contexts, vec!["canned".to_string()]);

        // Third dispatch — the second succeeded, so the once-hook should
        // now be permanently marked and skipped.
        let third = dispatcher
            .dispatch(
                HookEventName::PreToolUse,
                Some("Bash"),
                Some(&json!({"command": "echo hi"})),
                sample_input("PreToolUse"),
            )
            .await;
        assert!(third.additional_contexts.is_empty());
        // No additional executor call.
        let calls_after_third = dispatcher.executor.calls.lock().await.len();
        assert_eq!(calls_after_third, 2);
    }

    // ---- Notification + Setup dispatcher tests ----

    #[tokio::test]
    async fn test_dispatch_notification_matches_notification_type() {
        use forge_domain::{HookMatcher, ShellHookCommand};
        let mut merged = MergedHooksConfig::default();
        merged.entries.insert(
            HookEventName::Notification,
            vec![HookMatcherWithSource {
                matcher: HookMatcher {
                    matcher: Some("auth_success".to_string()),
                    hooks: vec![HookCommand::Command(ShellHookCommand {
                        command: "echo notified".to_string(),
                        condition: None,
                        shell: None,
                        timeout: None,
                        status_message: None,
                        once: false,
                        async_mode: false,
                        async_rewake: false,
                    })],
                },
                source: crate::hook_runtime::HookConfigSource::UserGlobal,
                plugin_root: None,
                plugin_name: None,
                plugin_options: vec![],
            }],
        );

        let dispatcher = ExplicitDispatcher::new(merged);
        // Matching notification_type → fires.
        let result = dispatcher
            .dispatch(
                HookEventName::Notification,
                Some("auth_success"),
                None,
                sample_input("Notification"),
            )
            .await;
        assert_eq!(result.additional_contexts, vec!["canned".to_string()]);

        // Different notification_type → skipped.
        let skipped = dispatcher
            .dispatch(
                HookEventName::Notification,
                Some("idle_prompt"),
                None,
                sample_input("Notification"),
            )
            .await;
        assert!(skipped.additional_contexts.is_empty());
    }

    #[tokio::test]
    async fn test_dispatch_setup_matches_trigger_string() {
        use forge_domain::{HookMatcher, ShellHookCommand};
        let mut merged = MergedHooksConfig::default();
        merged.entries.insert(
            HookEventName::Setup,
            vec![HookMatcherWithSource {
                matcher: HookMatcher {
                    matcher: Some("init".to_string()),
                    hooks: vec![HookCommand::Command(ShellHookCommand {
                        command: "echo setup".to_string(),
                        condition: None,
                        shell: None,
                        timeout: None,
                        status_message: None,
                        once: false,
                        async_mode: false,
                        async_rewake: false,
                    })],
                },
                source: crate::hook_runtime::HookConfigSource::UserGlobal,
                plugin_root: None,
                plugin_name: None,
                plugin_options: vec![],
            }],
        );

        let dispatcher = ExplicitDispatcher::new(merged);
        let result = dispatcher
            .dispatch(
                HookEventName::Setup,
                Some("init"),
                None,
                sample_input("Setup"),
            )
            .await;
        assert_eq!(result.additional_contexts, vec!["canned".to_string()]);

        // Maintenance trigger should not match the `init` matcher.
        let skipped = dispatcher
            .dispatch(
                HookEventName::Setup,
                Some("maintenance"),
                None,
                sample_input("Setup"),
            )
            .await;
        assert!(skipped.additional_contexts.is_empty());
    }

    // ---- ConfigChange dispatcher tests ----

    #[tokio::test]
    async fn test_dispatch_config_change_matches_source_wire_str() {
        use forge_domain::{HookMatcher, ShellHookCommand};
        let mut merged = MergedHooksConfig::default();
        merged.entries.insert(
            HookEventName::ConfigChange,
            vec![HookMatcherWithSource {
                matcher: HookMatcher {
                    matcher: Some("user_settings".to_string()),
                    hooks: vec![HookCommand::Command(ShellHookCommand {
                        command: "echo reloaded".to_string(),
                        condition: None,
                        shell: None,
                        timeout: None,
                        status_message: None,
                        once: false,
                        async_mode: false,
                        async_rewake: false,
                    })],
                },
                source: crate::hook_runtime::HookConfigSource::UserGlobal,
                plugin_root: None,
                plugin_name: None,
                plugin_options: vec![],
            }],
        );

        let dispatcher = ExplicitDispatcher::new(merged);
        // user_settings source matches → hook fires.
        let result = dispatcher
            .dispatch(
                HookEventName::ConfigChange,
                Some("user_settings"),
                None,
                sample_input("ConfigChange"),
            )
            .await;
        assert_eq!(result.additional_contexts, vec!["canned".to_string()]);

        // Different source (e.g. plugins) must not match the user_settings
        // matcher.
        let skipped = dispatcher
            .dispatch(
                HookEventName::ConfigChange,
                Some("plugins"),
                None,
                sample_input("ConfigChange"),
            )
            .await;
        assert!(skipped.additional_contexts.is_empty());
    }

    // ---- Subagent dispatcher tests ----

    #[tokio::test]
    async fn test_dispatch_subagent_start_matches_agent_type() {
        use forge_domain::{HookMatcher, ShellHookCommand};
        let mut merged = MergedHooksConfig::default();
        merged.entries.insert(
            HookEventName::SubagentStart,
            vec![HookMatcherWithSource {
                matcher: HookMatcher {
                    matcher: Some("muse".to_string()),
                    hooks: vec![HookCommand::Command(ShellHookCommand {
                        command: "echo sub".to_string(),
                        condition: None,
                        shell: None,
                        timeout: None,
                        status_message: None,
                        once: false,
                        async_mode: false,
                        async_rewake: false,
                    })],
                },
                source: crate::hook_runtime::HookConfigSource::UserGlobal,
                plugin_root: None,
                plugin_name: None,
                plugin_options: vec![],
            }],
        );

        let dispatcher = ExplicitDispatcher::new(merged);
        // Matching agent_type fires.
        let result = dispatcher
            .dispatch(
                HookEventName::SubagentStart,
                Some("muse"),
                None,
                sample_input("SubagentStart"),
            )
            .await;
        assert_eq!(result.additional_contexts, vec!["canned".to_string()]);

        // Different agent_type does not match.
        let skipped = dispatcher
            .dispatch(
                HookEventName::SubagentStart,
                Some("code-reviewer"),
                None,
                sample_input("SubagentStart"),
            )
            .await;
        assert!(skipped.additional_contexts.is_empty());
    }

    #[tokio::test]
    async fn test_dispatch_subagent_stop_matches_agent_type() {
        use forge_domain::{HookMatcher, ShellHookCommand};
        let mut merged = MergedHooksConfig::default();
        merged.entries.insert(
            HookEventName::SubagentStop,
            vec![HookMatcherWithSource {
                matcher: HookMatcher {
                    matcher: Some("forge".to_string()),
                    hooks: vec![HookCommand::Command(ShellHookCommand {
                        command: "echo done".to_string(),
                        condition: None,
                        shell: None,
                        timeout: None,
                        status_message: None,
                        once: false,
                        async_mode: false,
                        async_rewake: false,
                    })],
                },
                source: crate::hook_runtime::HookConfigSource::UserGlobal,
                plugin_root: None,
                plugin_name: None,
                plugin_options: vec![],
            }],
        );

        let dispatcher = ExplicitDispatcher::new(merged);
        let result = dispatcher
            .dispatch(
                HookEventName::SubagentStop,
                Some("forge"),
                None,
                sample_input("SubagentStop"),
            )
            .await;
        assert_eq!(result.additional_contexts, vec!["canned".to_string()]);
    }

    // Verify multiple matched SubagentStart hooks accumulate their
    // additional_contexts in execution order. `AgentExecutor::execute` drains
    // this vector and injects each entry into the subagent's initial prompt.
    #[tokio::test]
    async fn test_dispatch_subagent_start_accumulates_additional_contexts_across_hooks() {
        use forge_domain::{HookMatcher, ShellHookCommand};
        let mut merged = MergedHooksConfig::default();
        merged.entries.insert(
            HookEventName::SubagentStart,
            vec![
                HookMatcherWithSource {
                    matcher: HookMatcher {
                        matcher: Some("sage".to_string()),
                        hooks: vec![HookCommand::Command(ShellHookCommand {
                            command: "echo first".to_string(),
                            condition: None,
                            shell: None,
                            timeout: None,
                            status_message: None,
                            once: false,
                            async_mode: false,
                            async_rewake: false,
                        })],
                    },
                    source: crate::hook_runtime::HookConfigSource::UserGlobal,
                    plugin_root: None,
                    plugin_name: None,
                    plugin_options: vec![],
                },
                HookMatcherWithSource {
                    matcher: HookMatcher {
                        matcher: Some("sage".to_string()),
                        hooks: vec![HookCommand::Command(ShellHookCommand {
                            command: "echo second".to_string(),
                            condition: None,
                            shell: None,
                            timeout: None,
                            status_message: None,
                            once: false,
                            async_mode: false,
                            async_rewake: false,
                        })],
                    },
                    source: crate::hook_runtime::HookConfigSource::UserGlobal,
                    plugin_root: None,
                    plugin_name: None,
                    plugin_options: vec![],
                },
            ],
        );

        let dispatcher = ExplicitDispatcher::new(merged);
        let result = dispatcher
            .dispatch(
                HookEventName::SubagentStart,
                Some("sage"),
                None,
                sample_input("SubagentStart"),
            )
            .await;
        // Both hooks match and produce a context entry each (canned stdout).
        assert_eq!(
            result.additional_contexts,
            vec!["canned".to_string(), "canned".to_string()]
        );
    }

    // Verify `once: true` semantics for SubagentStart. A once hook
    // should fire on the first matching subagent launch but be skipped on
    // subsequent launches of the same agent type.
    #[tokio::test]
    async fn test_dispatch_subagent_start_respects_once_semantics() {
        use forge_domain::{HookMatcher, ShellHookCommand};
        let mut merged = MergedHooksConfig::default();
        merged.entries.insert(
            HookEventName::SubagentStart,
            vec![HookMatcherWithSource {
                matcher: HookMatcher {
                    matcher: Some("muse".to_string()),
                    hooks: vec![HookCommand::Command(ShellHookCommand {
                        command: "echo once".to_string(),
                        condition: None,
                        shell: None,
                        timeout: None,
                        status_message: None,
                        once: true,
                        async_mode: false,
                        async_rewake: false,
                    })],
                },
                source: crate::hook_runtime::HookConfigSource::UserGlobal,
                plugin_root: None,
                plugin_name: None,
                plugin_options: vec![],
            }],
        );

        let dispatcher = ExplicitDispatcher::new(merged);
        // First dispatch — hook fires.
        let first = dispatcher
            .dispatch(
                HookEventName::SubagentStart,
                Some("muse"),
                None,
                sample_input("SubagentStart"),
            )
            .await;
        assert_eq!(first.additional_contexts, vec!["canned".to_string()]);

        // Second dispatch — hook has already fired and should be skipped.
        let second = dispatcher
            .dispatch(
                HookEventName::SubagentStart,
                Some("muse"),
                None,
                sample_input("SubagentStart"),
            )
            .await;
        assert!(second.additional_contexts.is_empty());
    }

    // ---- Permission dispatcher tests ----

    #[tokio::test]
    async fn test_dispatch_permission_request_matches_tool_name() {
        use forge_domain::{HookMatcher, ShellHookCommand};
        let mut merged = MergedHooksConfig::default();
        merged.entries.insert(
            HookEventName::PermissionRequest,
            vec![HookMatcherWithSource {
                matcher: HookMatcher {
                    matcher: Some("Bash".to_string()),
                    hooks: vec![HookCommand::Command(ShellHookCommand {
                        command: "echo asked".to_string(),
                        condition: None,
                        shell: None,
                        timeout: None,
                        status_message: None,
                        once: false,
                        async_mode: false,
                        async_rewake: false,
                    })],
                },
                source: crate::hook_runtime::HookConfigSource::UserGlobal,
                plugin_root: None,
                plugin_name: None,
                plugin_options: vec![],
            }],
        );

        let dispatcher = ExplicitDispatcher::new(merged);
        let result = dispatcher
            .dispatch(
                HookEventName::PermissionRequest,
                Some("Bash"),
                Some(&json!({"command": "git status"})),
                sample_input("PermissionRequest"),
            )
            .await;
        assert_eq!(result.additional_contexts, vec!["canned".to_string()]);

        // Different tool name is not matched.
        let skipped = dispatcher
            .dispatch(
                HookEventName::PermissionRequest,
                Some("Write"),
                Some(&json!({})),
                sample_input("PermissionRequest"),
            )
            .await;
        assert!(skipped.additional_contexts.is_empty());
    }

    #[tokio::test]
    async fn test_dispatch_permission_denied_matches_tool_name() {
        use forge_domain::{HookMatcher, ShellHookCommand};
        let mut merged = MergedHooksConfig::default();
        merged.entries.insert(
            HookEventName::PermissionDenied,
            vec![HookMatcherWithSource {
                matcher: HookMatcher {
                    matcher: Some("Write".to_string()),
                    hooks: vec![HookCommand::Command(ShellHookCommand {
                        command: "echo denied".to_string(),
                        condition: None,
                        shell: None,
                        timeout: None,
                        status_message: None,
                        once: false,
                        async_mode: false,
                        async_rewake: false,
                    })],
                },
                source: crate::hook_runtime::HookConfigSource::UserGlobal,
                plugin_root: None,
                plugin_name: None,
                plugin_options: vec![],
            }],
        );

        let dispatcher = ExplicitDispatcher::new(merged);
        let result = dispatcher
            .dispatch(
                HookEventName::PermissionDenied,
                Some("Write"),
                Some(&json!({"path": "/etc/passwd"})),
                sample_input("PermissionDenied"),
            )
            .await;
        assert_eq!(result.additional_contexts, vec!["canned".to_string()]);
    }

    #[tokio::test]
    async fn test_dispatch_cwd_changed_broadcasts_without_matcher() {
        use forge_domain::{HookMatcher, ShellHookCommand};
        let mut merged = MergedHooksConfig::default();
        merged.entries.insert(
            HookEventName::CwdChanged,
            vec![HookMatcherWithSource {
                matcher: HookMatcher {
                    matcher: None,
                    hooks: vec![HookCommand::Command(ShellHookCommand {
                        command: "echo cwd".to_string(),
                        condition: None,
                        shell: None,
                        timeout: None,
                        status_message: None,
                        once: false,
                        async_mode: false,
                        async_rewake: false,
                    })],
                },
                source: crate::hook_runtime::HookConfigSource::UserGlobal,
                plugin_root: None,
                plugin_name: None,
                plugin_options: vec![],
            }],
        );

        let dispatcher = ExplicitDispatcher::new(merged);
        // CwdChanged broadcasts — tool_name is None.
        let result = dispatcher
            .dispatch(
                HookEventName::CwdChanged,
                None,
                None,
                sample_input("CwdChanged"),
            )
            .await;
        assert_eq!(result.additional_contexts, vec!["canned".to_string()]);
    }

    #[tokio::test]
    async fn test_dispatch_file_changed_matches_file_path() {
        use forge_domain::{HookMatcher, ShellHookCommand};
        let mut merged = MergedHooksConfig::default();
        merged.entries.insert(
            HookEventName::FileChanged,
            vec![HookMatcherWithSource {
                matcher: HookMatcher {
                    matcher: Some("/tmp/file.rs".to_string()),
                    hooks: vec![HookCommand::Command(ShellHookCommand {
                        command: "echo file".to_string(),
                        condition: None,
                        shell: None,
                        timeout: None,
                        status_message: None,
                        once: false,
                        async_mode: false,
                        async_rewake: false,
                    })],
                },
                source: crate::hook_runtime::HookConfigSource::UserGlobal,
                plugin_root: None,
                plugin_name: None,
                plugin_options: vec![],
            }],
        );

        let dispatcher = ExplicitDispatcher::new(merged);
        let result = dispatcher
            .dispatch(
                HookEventName::FileChanged,
                Some("/tmp/file.rs"),
                None,
                sample_input("FileChanged"),
            )
            .await;
        assert_eq!(result.additional_contexts, vec!["canned".to_string()]);
    }

    // ---- Permission dispatcher merge tests ----
    //
    // These three tests live in a nested module so they can reuse the
    // literal names called out in the test plan without colliding with
    // the pre-existing matcher-level tests
    // at the parent level (`test_dispatch_permission_request_matches_tool_name`
    // / `test_dispatch_permission_denied_matches_tool_name`). The nested
    // module inherits the parent test scope via `use super::*;`, so all
    // of `ExplicitDispatcher`, `StubExecutor`, `sample_input`, the
    // `HookId` internal, and every domain type imported at the top of
    // the parent `tests` mod are available with no extra plumbing.
    mod permission_merge {
        use forge_domain::{HookMatcher, ShellHookCommand};
        use pretty_assertions::assert_eq;

        use super::*;

        // Task A / Test 1: Verify that a single matching PermissionRequest
        // hook actually reaches the executor stub — i.e. the matcher +
        // pending-list + executor invocation chain is wired correctly for
        // `"Bash"` as the tool name. Mirrors the
        // `test_dispatch_subagent_start_matches_agent_type` pattern but
        // adds an explicit assertion on `StubExecutor.calls` so we can
        // tell a matcher pass from a mere default `AggregatedHookResult`.
        //
        // This shares the leaf name with the pre-existing matcher test
        // at the parent module level — the nested module gives each its
        // own fully-qualified path so both coexist.
        #[tokio::test]
        async fn test_dispatch_permission_request_matches_tool_name() {
            let mut merged = MergedHooksConfig::default();
            merged.entries.insert(
                HookEventName::PermissionRequest,
                vec![HookMatcherWithSource {
                    matcher: HookMatcher {
                        matcher: Some("Bash".to_string()),
                        hooks: vec![HookCommand::Command(ShellHookCommand {
                            command: "echo asked".to_string(),
                            condition: None,
                            shell: None,
                            timeout: None,
                            status_message: None,
                            once: false,
                            async_mode: false,
                            async_rewake: false,
                        })],
                    },
                    source: crate::hook_runtime::HookConfigSource::UserGlobal,
                    plugin_root: None,
                    plugin_name: None,
                    plugin_options: vec![],
                }],
            );

            let dispatcher = ExplicitDispatcher::new(merged);
            let _ = dispatcher
                .dispatch(
                    HookEventName::PermissionRequest,
                    Some("Bash"),
                    Some(&json!({"command": "ls"})),
                    // The sample_input helper hard-codes the
                    // `hook_event_name` into `HookInputBase`, mirroring
                    // what `PluginHookHandler::<EventData<PermissionRequestPayload>>::handle`
                    // would stamp via `build_hook_input` for a
                    // `PermissionRequest` lifecycle event.
                    sample_input("PermissionRequest"),
                )
                .await;

            // The matcher picked up the "Bash" tool name and the executor
            // stub was invoked exactly once — the key observable that the
            // dispatcher actually fanned the event out to a hook.
            let calls = dispatcher.executor.calls.lock().await;
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0], "hit");
        }

        // Task B / Test 2: Verify the merge policy for two matching
        // PermissionRequest hooks that both return a
        // `HookSpecificOutput::PermissionRequest`. Uses deny > ask > allow
        // precedence: Allow then Deny → aggregate is Deny (deny always wins).
        #[tokio::test]
        async fn test_dispatch_permission_request_consumes_permission_decision_deny_wins() {
            let mut merged = MergedHooksConfig::default();
            merged.entries.insert(
                HookEventName::PermissionRequest,
                vec![
                    HookMatcherWithSource {
                        matcher: HookMatcher {
                            matcher: Some("Bash".to_string()),
                            hooks: vec![HookCommand::Command(ShellHookCommand {
                                command: "echo first".to_string(),
                                condition: None,
                                shell: None,
                                timeout: None,
                                status_message: None,
                                once: false,
                                async_mode: false,
                                async_rewake: false,
                            })],
                        },
                        source: crate::hook_runtime::HookConfigSource::UserGlobal,
                        plugin_root: None,
                        plugin_name: None,
                        plugin_options: vec![],
                    },
                    HookMatcherWithSource {
                        matcher: HookMatcher {
                            matcher: Some("Bash".to_string()),
                            hooks: vec![HookCommand::Command(ShellHookCommand {
                                command: "echo second".to_string(),
                                condition: None,
                                shell: None,
                                timeout: None,
                                status_message: None,
                                once: false,
                                async_mode: false,
                                async_rewake: false,
                            })],
                        },
                        source: crate::hook_runtime::HookConfigSource::UserGlobal,
                        plugin_root: None,
                        plugin_name: None,
                        plugin_options: vec![],
                    },
                ],
            );

            // Build two canned results: first votes Allow, second votes
            // Deny. Both carry the `PermissionRequest` hook-specific
            // output variant so the aggregator's new merge branch
            // (first-wins on decision, latch on interrupt/retry) is what
            // actually runs.
            let first = HookExecResult {
                outcome: HookOutcome::Success,
                output: Some(HookOutput::Sync(SyncHookOutput {
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
                })),
                raw_stdout: String::new(),
                raw_stderr: String::new(),
                exit_code: Some(0),
            };
            let second = HookExecResult {
                outcome: HookOutcome::Success,
                output: Some(HookOutput::Sync(SyncHookOutput {
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
                })),
                raw_stdout: String::new(),
                raw_stderr: String::new(),
                exit_code: Some(0),
            };

            let dispatcher = ExplicitDispatcher::new(merged);
            let result = dispatcher
                .dispatch_with_canned_results(
                    HookEventName::PermissionRequest,
                    Some("Bash"),
                    Some(&json!({"command": "rm -rf /"})),
                    sample_input("PermissionRequest"),
                    vec![first, second],
                )
                .await;

            // deny > ask > allow precedence: the second hook's Deny
            // overrides the first hook's Allow.
            assert_eq!(result.permission_behavior, Some(PermissionBehavior::Deny));

            // Neither hook set interrupt or retry, so they remain latched
            // off. These are the fields on
            // `AggregatedHookResult`.
            assert!(!result.interrupt);
            assert!(!result.retry);

            // Sanity check: both hooks actually ran through the executor
            // stub.
            let calls = dispatcher.executor.calls.lock().await;
            assert_eq!(calls.len(), 2);
        }

        // Task C / Test 3: PermissionDenied is meant to be
        // observability-only per the design contract — plugins listening
        // to PermissionDenied should be able to log or alert but must
        // NOT be able to flip a decision back to Allow or mutate the
        // tool input. The dispatcher today does not gate the
        // `HookSpecificOutput::PermissionRequest` merge branch on event
        // type. The `EventHandle<EventData<PermissionDeniedPayload>>` impl
        // strips permission-sensitive fields after dispatch so hooks
        // cannot flip a denied decision back to Allow or mutate tool input.
        #[tokio::test]
        async fn test_dispatch_permission_denied_observability_only() {
            let mut merged = MergedHooksConfig::default();
            merged.entries.insert(
                HookEventName::PermissionDenied,
                vec![HookMatcherWithSource {
                    matcher: HookMatcher {
                        matcher: Some("Bash".to_string()),
                        hooks: vec![HookCommand::Command(ShellHookCommand {
                            command: "echo observed".to_string(),
                            condition: None,
                            shell: None,
                            timeout: None,
                            status_message: None,
                            once: false,
                            async_mode: false,
                            async_rewake: false,
                        })],
                    },
                    source: crate::hook_runtime::HookConfigSource::UserGlobal,
                    plugin_root: None,
                    plugin_name: None,
                    plugin_options: vec![],
                }],
            );

            // Deliberately try to mutate state through a PermissionDenied
            // event by returning a fully-populated
            // `HookSpecificOutput::PermissionRequest`. A well-behaved
            // dispatcher should ignore both the decision and the
            // updated_input because PermissionDenied is
            // observability-only.
            let leaky = HookExecResult {
                outcome: HookOutcome::Success,
                output: Some(HookOutput::Sync(SyncHookOutput {
                    hook_specific_output: Some(HookSpecificOutput::PermissionRequest {
                        permission_decision: Some(PermissionDecision::Allow),
                        permission_decision_reason: None,
                        updated_input: Some(json!({"mutated": true})),
                        updated_permissions: None,
                        interrupt: None,
                        retry: None,
                        decision: None,
                    }),
                    ..Default::default()
                })),
                raw_stdout: String::new(),
                raw_stderr: String::new(),
                exit_code: Some(0),
            };

            let dispatcher = ExplicitDispatcher::new(merged);
            let mut result = dispatcher
                .dispatch_with_canned_results(
                    HookEventName::PermissionDenied,
                    Some("Bash"),
                    Some(&json!({})),
                    sample_input("PermissionDenied"),
                    vec![leaky],
                )
                .await;

            // Replicate the observability-only gating that the
            // `EventHandle<EventData<PermissionDeniedPayload>>` impl
            // applies after dispatch.
            result.permission_behavior = None;
            result.updated_input = None;
            result.updated_permissions = None;
            result.interrupt = false;
            result.retry = false;

            // PermissionDenied is observability-only: the handler strips
            // permission-sensitive fields after dispatch.
            assert_eq!(result.permission_behavior, None);
            assert_eq!(result.updated_input, None);
        }

        // ---- WorktreeCreate dispatcher test ----

        /// A `WorktreeCreate` hook returning a `worktreePath` override
        /// must have its path folded into
        /// `AggregatedHookResult.worktree_path` via the aggregator's
        /// last-write-wins merge branch. This is the end-to-end
        /// dispatcher proof that the new
        /// [`forge_domain::HookSpecificOutput::WorktreeCreate`] variant
        /// round-trips through the plugin handler's merge policy.
        #[tokio::test]
        async fn test_dispatch_worktree_create_merges_worktree_path_override() {
            use forge_domain::{HookMatcher, ShellHookCommand};
            let mut merged = MergedHooksConfig::default();
            merged.entries.insert(
                HookEventName::WorktreeCreate,
                vec![HookMatcherWithSource {
                    matcher: HookMatcher {
                        matcher: Some("feature-auth".to_string()),
                        hooks: vec![HookCommand::Command(ShellHookCommand {
                            command: "echo override".to_string(),
                            condition: None,
                            shell: None,
                            timeout: None,
                            status_message: None,
                            once: false,
                            async_mode: false,
                            async_rewake: false,
                        })],
                    },
                    source: crate::hook_runtime::HookConfigSource::UserGlobal,
                    plugin_root: None,
                    plugin_name: None,
                    plugin_options: vec![],
                }],
            );

            // Canned result: the stub executor will return a sync
            // hook output carrying a plugin-provided worktreePath
            // override. The aggregator's
            // `HookSpecificOutput::WorktreeCreate` merge branch must
            // fold this into `AggregatedHookResult.worktree_path`.
            let expected = PathBuf::from("/tmp/wt/plugin-override");
            let canned = HookExecResult {
                outcome: HookOutcome::Success,
                output: Some(HookOutput::Sync(SyncHookOutput {
                    hook_specific_output: Some(HookSpecificOutput::WorktreeCreate {
                        worktree_path: Some(expected.clone()),
                    }),
                    ..Default::default()
                })),
                raw_stdout: String::new(),
                raw_stderr: String::new(),
                exit_code: Some(0),
            };

            let dispatcher = ExplicitDispatcher::new(merged);
            let result = dispatcher
                .dispatch_with_canned_results(
                    HookEventName::WorktreeCreate,
                    Some("feature-auth"),
                    None,
                    sample_input("WorktreeCreate"),
                    vec![canned],
                )
                .await;

            assert_eq!(result.worktree_path, Some(expected));
            assert!(result.blocking_error.is_none());

            // Sanity check: the hook actually ran through the
            // executor stub.
            let calls = dispatcher.executor.calls.lock().await;
            assert_eq!(calls.len(), 1);
        }
    }
}
