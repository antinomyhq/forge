use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_recursion::async_recursion;
use derive_setters::Setters;
use forge_domain::{Agent, *};
use forge_template::Element;
use futures::future::join_all;
use notify_debouncer_full::notify::RecursiveMode;
use tokio::sync::Notify;
use tracing::warn;

use crate::agent::AgentService;
use crate::async_hook_queue::AsyncHookResultQueue;
use crate::lifecycle_fires::add_file_changed_watch_paths;
use crate::{EnvironmentInfra, TemplateEngine};

#[derive(Clone, Setters)]
#[setters(into)]
pub struct Orchestrator<S> {
    services: Arc<S>,
    sender: Option<ArcSender>,
    conversation: Conversation,
    tool_definitions: Vec<ToolDefinition>,
    models: Vec<Model>,
    agent: Agent,
    error_tracker: ToolErrorTracker,
    hook: Arc<Hook>,
    config: forge_config::ForgeConfig,
    /// Optional most-recent user prompt text. Used to populate the
    /// `UserPromptSubmit` hook payload fired on the first iteration of
    /// [`Orchestrator::run`]. Callers set it via the derived
    /// [`Orchestrator::user_prompt`] setter.
    #[setters(into, strip_option)]
    user_prompt: Option<String>,
    /// Shared queue for async-rewake hook results. The orchestrator
    /// drains this queue before each conversation turn and injects
    /// pending results as `<system_reminder>` context messages —
    /// mirroring Claude Code's `enqueuePendingNotification` pipeline.
    #[setters(into, strip_option)]
    async_hook_queue: Option<AsyncHookResultQueue>,
}

impl<S: AgentService + EnvironmentInfra<Config = forge_config::ForgeConfig>> Orchestrator<S> {
    pub fn new(
        services: Arc<S>,
        conversation: Conversation,
        agent: Agent,
        config: forge_config::ForgeConfig,
    ) -> Self {
        Self {
            conversation,
            services,
            agent,
            config,
            sender: Default::default(),
            tool_definitions: Default::default(),
            models: Default::default(),
            error_tracker: Default::default(),
            hook: Arc::new(Hook::default()),
            user_prompt: None,
            async_hook_queue: None,
        }
    }

    /// Get a reference to the internal conversation
    pub fn get_conversation(&self) -> &Conversation {
        &self.conversation
    }

    /// Resolve the plugin-hook context tuple (session_id, transcript_path,
    /// cwd) for the current conversation. Used by every fire site to
    /// build [`EventData::with_context`] without duplicating the lookup.
    ///
    /// TODO(subagent-threading): When the Orchestrator runs inside a
    /// subagent, every event it fires should carry the subagent's
    /// UUID in the wire-level `HookInputBase.agent_id` instead of
    /// the main conversation's agent id. Implementing this cleanly
    /// is invasive — it requires adding `current_subagent_id:
    /// Option<String>` to `Orchestrator`, threading it via either
    /// `ChatRequest` or `Conversation`, plumbing a new
    /// `subagent_id: Option<String>` field through `EventData` and
    /// `PluginHookHandler::build_hook_input`, and updating every
    /// fire site in `orch.rs` that currently destructures the
    /// 3-tuple from this helper. The explicit `SubagentStart` /
    /// `SubagentStop` fire sites at the executor boundary carry the
    /// subagent UUID directly inside the payload so plugins that
    /// need to distinguish main-vs-subagent context can still
    /// filter on them today. Full inner-orchestrator threading can
    /// be revisited if a use case materializes.
    fn plugin_hook_context(&self) -> (String, PathBuf, PathBuf) {
        let session_id = self.conversation.id.into_string();
        let environment = self.services.get_environment();
        let transcript_path = environment.transcript_path(&session_id);
        let cwd = environment.cwd.clone();
        (session_id, transcript_path, cwd)
    }

    // Helper function to get all tool results from a vector of tool calls
    #[async_recursion]
    #[allow(deprecated)]
    async fn execute_tool_calls(
        &mut self,
        tool_calls: &[ToolCallFull],
        tool_context: &ToolCallContext,
    ) -> anyhow::Result<Vec<(ToolCallFull, ToolResult)>> {
        let task_tool_name = ToolKind::Task.name();

        // Use a case-insensitive comparison since the model may send "Task" or "task".
        let is_task = |tc: &ToolCallFull| {
            tc.name
                .as_str()
                .eq_ignore_ascii_case(task_tool_name.as_str())
        };

        // Partition into task tool calls (run in parallel) and all others (run
        // sequentially). Use a case-insensitive comparison since the model may
        // send "Task" or "task".
        let is_task_call =
            |tc: &&ToolCallFull| tc.name.as_str().to_lowercase() == task_tool_name.as_str();
        let (task_calls, other_calls): (Vec<_>, Vec<_>) = tool_calls.iter().partition(is_task_call);

        // Execute task tool calls in parallel — mirrors how direct agent-as-tool calls
        // work.
        let task_results: Vec<(ToolCallFull, ToolResult)> = join_all(
            task_calls
                .iter()
                .map(|tc| self.services.call(&self.agent, tool_context, (*tc).clone())),
        )
        .await
        .into_iter()
        .zip(task_calls.iter())
        .map(|(result, tc)| ((*tc).clone(), result))
        .collect();

        let system_tools = self
            .tool_definitions
            .iter()
            .map(|tool| &tool.name)
            .collect::<HashSet<_>>();

        // Resolve plugin-hook context once per tool-call batch. The
        // same values are used when firing PreToolUse / PostToolUse /
        // PostToolUseFailure hooks.
        let (session_id, transcript_path, cwd) = self.plugin_hook_context();

        // Process non-task tool calls sequentially (preserving UI notifier handshake
        // and hooks).
        let mut other_results: Vec<(ToolCallFull, ToolResult)> =
            Vec::with_capacity(other_calls.len());
        for tool_call in &other_calls {
            // Send the start notification for system tools and not agent as a tool
            let is_system_tool = system_tools.contains(&tool_call.name);
            if is_system_tool {
                let notifier = Arc::new(Notify::new());
                self.send(ChatResponse::ToolCallStart {
                    tool_call: (*tool_call).clone(),
                    notifier: notifier.clone(),
                })
                .await?;
                // Wait for the UI to acknowledge it has rendered the tool header
                // before we execute the tool. This prevents tool stdout from
                // appearing before the tool name is printed.
                notifier.notified().await;
            }

            // Fire the ToolcallStart lifecycle event
            let toolcall_start_event = LifecycleEvent::ToolcallStart(EventData::with_context(
                self.agent.clone(),
                self.agent.model.clone(),
                session_id.clone(),
                transcript_path.clone(),
                cwd.clone(),
                ToolcallStartPayload::new((*tool_call).clone()),
            ));
            self.hook
                .handle(&toolcall_start_event, &mut self.conversation)
                .await?;

            // Fire PreToolUse (Claude Code plugin event)
            self.conversation.reset_hook_result();
            let pre_tool_use_payload = PreToolUsePayload {
                tool_name: tool_call.name.as_str().to_string(),
                tool_input: serde_json::to_value(&tool_call.arguments).unwrap_or_default(),
                tool_use_id: tool_call
                    .call_id
                    .as_ref()
                    .map(|id| id.as_str().to_string())
                    .unwrap_or_default(),
            };
            let pre_tool_use_event = LifecycleEvent::PreToolUse(EventData::with_context(
                self.agent.clone(),
                self.agent.model.clone(),
                session_id.clone(),
                transcript_path.clone(),
                cwd.clone(),
                pre_tool_use_payload,
            ));
            self.hook
                .handle(&pre_tool_use_event, &mut self.conversation)
                .await?;

            // Consume PreToolUse hook_result:
            //  1. blocking_error OR permission_behavior==Deny → synthesize error ToolResult
            //     and skip services.call()
            //  2. additional_contexts → push as <system_reminder> into context
            //  3. updated_input → override tool_call.arguments for this call
            let pre_hook_result = std::mem::take(&mut self.conversation.hook_result);

            // Inject additional_contexts as <system_reminder> messages
            if !pre_hook_result.additional_contexts.is_empty()
                && let Some(ctx) = self.conversation.context.as_mut()
            {
                for extra in &pre_hook_result.additional_contexts {
                    let wrapped = Element::new("system_reminder").text(extra);
                    ctx.messages.push(
                        ContextMessage::system_reminder(wrapped, Some(self.agent.model.clone()))
                            .into(),
                    );
                }
            }

            // Determine if PreToolUse blocked execution
            let is_denied = matches!(
                pre_hook_result.permission_behavior,
                Some(PermissionBehavior::Deny)
            );
            let block_reason: Option<String> = if let Some(err) = pre_hook_result.blocking_error {
                Some(err.message)
            } else if is_denied {
                Some("Tool call denied by plugin hook".to_string())
            } else {
                None
            };

            let tool_result = if let Some(reason) = block_reason {
                // Synthesize a failure ToolResult without calling services.call
                ToolResult::from((*tool_call).clone()).failure(anyhow::anyhow!("{}", reason))
            } else {
                // Apply updated_input if present
                let effective_call = if let Some(updated) = pre_hook_result.updated_input {
                    let mut ec = (*tool_call).clone();
                    ec.arguments = ToolCallArguments::from(updated);
                    ec
                } else {
                    (*tool_call).clone()
                };
                // Execute the tool
                self.services
                    .call(&self.agent, tool_context, effective_call)
                    .await
            };

            // Fire the ToolcallEnd lifecycle event (fires on both success and failure)
            let toolcall_end_event = LifecycleEvent::ToolcallEnd(EventData::with_context(
                self.agent.clone(),
                self.agent.model.clone(),
                session_id.clone(),
                transcript_path.clone(),
                cwd.clone(),
                ToolcallEndPayload::new((*tool_call).clone(), tool_result.clone()),
            ));
            self.hook
                .handle(&toolcall_end_event, &mut self.conversation)
                .await?;

            // Fire PostToolUse or PostToolUseFailure (demux on is_error)
            self.conversation.reset_hook_result();
            let tool_input = serde_json::to_value(&tool_call.arguments).unwrap_or_default();
            let tool_use_id = tool_call
                .call_id
                .as_ref()
                .map(|id| id.as_str().to_string())
                .unwrap_or_default();

            if tool_result.is_error() {
                let failure_payload = PostToolUseFailurePayload {
                    tool_name: tool_call.name.as_str().to_string(),
                    tool_input,
                    tool_use_id,
                    error: tool_result.output.as_str().unwrap_or_default().to_string(),
                    is_interrupt: None,
                };
                let event = LifecycleEvent::PostToolUseFailure(EventData::with_context(
                    self.agent.clone(),
                    self.agent.model.clone(),
                    session_id.clone(),
                    transcript_path.clone(),
                    cwd.clone(),
                    failure_payload,
                ));
                self.hook.handle(&event, &mut self.conversation).await?;
            } else {
                let tool_response = serde_json::to_value(&tool_result.output).unwrap_or_default();
                let post_payload = PostToolUsePayload {
                    tool_name: tool_call.name.as_str().to_string(),
                    tool_input,
                    tool_response,
                    tool_use_id,
                };
                let event = LifecycleEvent::PostToolUse(EventData::with_context(
                    self.agent.clone(),
                    self.agent.model.clone(),
                    session_id.clone(),
                    transcript_path.clone(),
                    cwd.clone(),
                    post_payload,
                ));
                self.hook.handle(&event, &mut self.conversation).await?;
            }

            // Consume PostToolUse hook_result:
            //  - additional_contexts → push as <system_reminder>
            //  - updated_mcp_tool_output → replace tool_result.output text
            let post_hook_result = std::mem::take(&mut self.conversation.hook_result);

            if !post_hook_result.additional_contexts.is_empty()
                && let Some(ctx) = self.conversation.context.as_mut()
            {
                for extra in &post_hook_result.additional_contexts {
                    let wrapped = Element::new("system_reminder").text(extra);
                    ctx.messages.push(
                        ContextMessage::system_reminder(wrapped, Some(self.agent.model.clone()))
                            .into(),
                    );
                }
            }

            // Apply updated_mcp_tool_output override if present (simple
            // text replacement of the tool's output values)
            let tool_result = if let Some(override_value) = post_hook_result.updated_mcp_tool_output
            {
                let text = serde_json::to_string(&override_value)
                    .unwrap_or_else(|_| override_value.to_string());
                let mut rewritten = tool_result.clone();
                rewritten.output = ToolOutput::text(text);
                rewritten
            } else {
                tool_result
            };

            // Send the end notification for system tools and not agent as a tool
            if is_system_tool {
                self.send(ChatResponse::ToolCallEnd(tool_result.clone()))
                    .await?;
            }
            other_results.push(((*tool_call).clone(), tool_result));
        }

        // Reconstruct results in the original order of tool_calls.
        let mut task_iter = task_results.into_iter();
        let mut other_iter = other_results.into_iter();
        let tool_call_records = tool_calls
            .iter()
            .map(|tc| {
                if is_task(tc) {
                    task_iter.next().expect("task result count mismatch")
                } else {
                    other_iter.next().expect("other result count mismatch")
                }
            })
            .collect();

        Ok(tool_call_records)
    }

    async fn send(&self, message: ChatResponse) -> anyhow::Result<()> {
        if let Some(sender) = &self.sender {
            sender.send(Ok(message)).await?
        }
        Ok(())
    }

    // Returns if agent supports tool or not.
    fn is_tool_supported(&self) -> anyhow::Result<bool> {
        let model_id = &self.agent.model;

        // Check if at agent level tool support is defined
        let tool_supported = match self.agent.tool_supported {
            Some(tool_supported) => tool_supported,
            None => {
                // If not defined at agent level, check model level

                let model = self.models.iter().find(|model| &model.id == model_id);
                model
                    .and_then(|model| model.tools_supported)
                    .unwrap_or_default()
            }
        };

        Ok(tool_supported)
    }

    async fn execute_chat_turn(
        &self,
        model_id: &ModelId,
        context: Context,
        reasoning_supported: bool,
    ) -> anyhow::Result<ChatCompletionMessageFull> {
        let tool_supported = self.is_tool_supported()?;
        let mut transformers = DefaultTransformation::default()
            .pipe(SortTools::new(self.agent.tool_order()))
            .pipe(NormalizeToolCallArguments::new())
            .pipe(TransformToolCalls::new().when(|_| !tool_supported))
            .pipe(ImageHandling::new())
            // Drop ALL reasoning (including config) when reasoning is not supported by the model
            .pipe(DropReasoningDetails.when(|_| !reasoning_supported))
            // Strip all reasoning from messages when the model has changed (signatures are
            // model-specific and invalid across models). No-op when model is unchanged.
            .pipe(ReasoningNormalizer::new(model_id.clone()));
        let response = self
            .services
            .chat_agent(
                model_id,
                transformers.transform(context),
                Some(self.agent.provider.clone()),
            )
            .await?;

        // Always stream content deltas
        response
            .into_full_streaming(!tool_supported, self.sender.clone())
            .await
    }

    // Public entry point that wraps `run_inner` so we can fire the
    // Claude Code `StopFailure` plugin event when the main loop halts
    // with an error. The StopFailure dispatch is best-effort: we
    // intentionally ignore any secondary error produced by the hook
    // handler so the original failure keeps its context as it
    // propagates back to the caller.
    pub async fn run(&mut self) -> anyhow::Result<()> {
        match self.run_inner().await {
            Ok(()) => Ok(()),
            Err(err) => {
                let (session_id, transcript_path, cwd) = self.plugin_hook_context();
                self.conversation.reset_hook_result();
                let stop_failure_payload = StopFailurePayload {
                    error: format!("{:#}", err),
                    error_details: None,
                    last_assistant_message: None,
                };
                let stop_failure_event = LifecycleEvent::StopFailure(EventData::with_context(
                    self.agent.clone(),
                    self.agent.model.clone(),
                    session_id,
                    transcript_path,
                    cwd,
                    stop_failure_payload,
                ));
                // Fire as best-effort — swallow any secondary hook error so
                // the original failure's context is preserved.
                let _ = self
                    .hook
                    .handle(&stop_failure_event, &mut self.conversation)
                    .await;
                let _ = std::mem::take(&mut self.conversation.hook_result);
                Err(err)
            }
        }
    }

    // Core orchestration loop. All existing `run` behavior lives here;
    // the public `run` wrapper adds `StopFailure` fire-site dispatch on
    // error.
    #[allow(deprecated)]
    async fn run_inner(&mut self) -> anyhow::Result<()> {
        let model_id = self.get_model();

        let mut context = self.conversation.context.clone().unwrap_or_default();

        // Resolve plugin-hook context (session id, transcript path, cwd)
        // once per `run` invocation. Every fire site below uses
        // `EventData::with_context` so the plugin hook dispatcher sees
        // real values instead of legacy sentinels.
        let (session_id, transcript_path, cwd) = self.plugin_hook_context();

        // Ensure the transcript directory + file exist before any hooks run.
        // This is a best-effort touch so external hook subprocesses can
        // append to the transcript file without first having to create it.
        if let Some(parent) = transcript_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&transcript_path);

        // Fire SessionStart (Claude Code plugin event) before any legacy
        // lifecycle event so plugins can inject `initial_user_message` /
        // additional contexts that the rest of the turn will see.
        self.conversation.reset_hook_result();
        let session_source = if context.messages.is_empty() {
            SessionStartSource::Startup
        } else {
            SessionStartSource::Resume
        };
        let session_start_payload = SessionStartPayload {
            source: session_source,
            model: Some(model_id.as_str().to_string()),
        };
        let session_start_event = LifecycleEvent::SessionStart(EventData::with_context(
            self.agent.clone(),
            model_id.clone(),
            session_id.clone(),
            transcript_path.clone(),
            cwd.clone(),
            session_start_payload,
        ));
        self.hook
            .handle(&session_start_event, &mut self.conversation)
            .await?;

        // Consume SessionStart hook_result:
        //  - initial_user_message → push as a User ContextMessage
        //  - additional_contexts → push as <system_reminder> messages
        //  - watch_paths → install runtime FileChanged watchers
        let session_start_hook_result = std::mem::take(&mut self.conversation.hook_result);

        if let Some(init_msg) = session_start_hook_result.initial_user_message {
            context
                .messages
                .push(ContextMessage::user(init_msg, Some(model_id.clone())).into());
        }

        if !session_start_hook_result.additional_contexts.is_empty() {
            for extra in &session_start_hook_result.additional_contexts {
                let wrapped = Element::new("system_reminder").text(extra);
                context
                    .messages
                    .push(ContextMessage::system_reminder(wrapped, Some(model_id.clone())).into());
            }
        }

        // Forward any dynamic watch_paths returned by the
        // `SessionStart` hook into the running `FileChangedWatcher`.
        //
        // Wire semantics: per Claude Code, a
        // `hookSpecificOutput.SessionStart.watch_paths` entry is a
        // single-file path (not a glob) that the watcher should observe
        // from that point forward. We assume the hook returned absolute
        // paths (the `HookSpecificOutput::SessionStart` serde shape is
        // `Vec<PathBuf>`), but guard against a relative entry by
        // resolving it against the current cwd — the alternative of
        // silently dropping relative entries would be harder to debug.
        //
        // All entries are installed as `NonRecursive` to match the
        // startup resolver's treatment of `FileChanged` matchers as
        // single-file targets. The dispatcher itself is a no-op if
        // `ForgeAPI::init` did not install a watcher (e.g. unit tests
        // or single-thread runtimes), so this call is safe to make
        // unconditionally.
        if !session_start_hook_result.watch_paths.is_empty() {
            let resolved: Vec<(PathBuf, RecursiveMode)> = session_start_hook_result
                .watch_paths
                .iter()
                .map(|p| {
                    let path = if p.is_absolute() {
                        p.clone()
                    } else {
                        cwd.join(p)
                    };
                    (path, RecursiveMode::NonRecursive)
                })
                .collect();

            tracing::debug!(
                count = resolved.len(),
                "SessionStart: adding runtime watch paths from hook output"
            );

            add_file_changed_watch_paths(resolved);
        }

        // Sync updated context back to the conversation so the legacy
        // Start event (and every subsequent handler) sees SessionStart's
        // injections.
        self.conversation.context = Some(context.clone());

        // Fire the Start lifecycle event
        let start_event = LifecycleEvent::Start(EventData::with_context(
            self.agent.clone(),
            model_id.clone(),
            session_id.clone(),
            transcript_path.clone(),
            cwd.clone(),
            StartPayload,
        ));
        self.hook
            .handle(&start_event, &mut self.conversation)
            .await?;

        // Signals that the loop should suspend (task may or may not be completed)
        let mut should_yield = false;

        // Signals that the task is completed
        let mut is_complete = false;

        // Tracks the most recent assistant message content. Used by the
        // Claude Code `Stop` plugin event to populate `last_assistant_message`.
        #[allow(unused_assignments)]
        let mut last_assistant_content: Option<String> = None;

        let mut request_count = 0;

        // Retrieve the number of requests allowed per tick.
        let max_requests_per_turn = self.agent.max_requests_per_turn;
        let tool_context =
            ToolCallContext::new(self.conversation.metrics.clone()).sender(self.sender.clone());

        while !should_yield {
            // Drain any pending async-rewake hook results and inject them
            // as <system_reminder> messages so the LLM sees them on the
            // current turn. This mirrors Claude Code's
            // `enqueuePendingNotification` + `queued_command` attachment
            // pipeline.
            if let Some(queue) = &self.async_hook_queue {
                let pending = queue.drain().await;
                for result in pending {
                    let prefix = if result.is_blocking { "BLOCKING: " } else { "" };
                    let text = format!(
                        "Async hook '{}' completed: {}{}",
                        result.hook_name, prefix, result.message
                    );
                    let wrapped = Element::new("system_reminder").text(&text);
                    context.messages.push(
                        ContextMessage::system_reminder(wrapped, Some(model_id.clone())).into(),
                    );
                }
            }

            // Set context for the current loop iteration
            self.conversation.context = Some(context.clone());
            self.services.update(self.conversation.clone()).await?;

            // Fire UserPromptSubmit on the first iteration only. Plugin
            // hooks can inject <system_reminder> additional contexts or
            // hard-block the turn via blocking_error.
            if request_count == 0
                && let Some(prompt_text) = self.user_prompt.clone()
            {
                self.conversation.reset_hook_result();
                let prompt_payload = UserPromptSubmitPayload { prompt: prompt_text };
                let prompt_event = LifecycleEvent::UserPromptSubmit(EventData::with_context(
                    self.agent.clone(),
                    model_id.clone(),
                    session_id.clone(),
                    transcript_path.clone(),
                    cwd.clone(),
                    prompt_payload,
                ));
                self.hook
                    .handle(&prompt_event, &mut self.conversation)
                    .await?;

                let prompt_hook_result = std::mem::take(&mut self.conversation.hook_result);

                // Inject additional_contexts as <system_reminder> messages
                if !prompt_hook_result.additional_contexts.is_empty() {
                    for extra in &prompt_hook_result.additional_contexts {
                        let wrapped = Element::new("system_reminder").text(extra);
                        context.messages.push(
                            ContextMessage::system_reminder(wrapped, Some(model_id.clone())).into(),
                        );
                    }
                    // Sync back before the Request event runs
                    self.conversation.context = Some(context.clone());
                }

                // A UserPromptSubmit hook can hard-block the turn.
                if let Some(err) = prompt_hook_result.blocking_error {
                    warn!(
                        agent_id = %self.agent.id,
                        error = %err.message,
                        "UserPromptSubmit hook blocked prompt"
                    );
                    return Ok(());
                }
            }

            let request_event = LifecycleEvent::Request(EventData::with_context(
                self.agent.clone(),
                model_id.clone(),
                session_id.clone(),
                transcript_path.clone(),
                cwd.clone(),
                RequestPayload::new(request_count),
            ));
            self.hook
                .handle(&request_event, &mut self.conversation)
                .await?;

            // Sync any mutations the request hook performed on
            // `self.conversation.context` back into the local `context`
            // so the next LLM call sees them. This enables hooks like
            // [`DoomLoopDetector`] and [`SkillListingHandler`] to inject
            // `<system_reminder>` messages that are visible on the current
            // turn (rather than being delayed by one iteration).
            if let Some(updated) = self.conversation.context.clone() {
                context = updated;
            }

            let message = crate::retry::retry_with_config(
                &self.config.clone().retry.unwrap_or_default(),
                || {
                    self.execute_chat_turn(
                        &model_id,
                        context.clone(),
                        context.is_reasoning_supported(),
                    )
                },
                self.sender.as_ref().map(|sender| {
                    let sender = sender.clone();
                    let agent_id = self.agent.id.clone();
                    let model_id = model_id.clone();
                    move |error: &anyhow::Error, duration: Duration| {
                        let root_cause = error.root_cause();
                        // Log retry attempts - critical for debugging API failures
                        tracing::error!(
                            agent_id = %agent_id,
                            error = ?root_cause,
                            model = %model_id,
                            "Retry attempt due to error"
                        );
                        let retry_event =
                            ChatResponse::RetryAttempt { cause: error.into(), duration };
                        let _ = sender.try_send(Ok(retry_event));
                    }
                }),
            )
            .await?;

            // Fire the Response lifecycle event
            let response_event = LifecycleEvent::Response(EventData::with_context(
                self.agent.clone(),
                model_id.clone(),
                session_id.clone(),
                transcript_path.clone(),
                cwd.clone(),
                ResponsePayload::new(message.clone()),
            ));
            self.hook
                .handle(&response_event, &mut self.conversation)
                .await?;

            // Capture for Stop payload
            last_assistant_content = Some(message.content.clone());

            // Turn is completed, if finish_reason is 'stop'. Gemini models return stop as
            // finish reason with tool calls.
            is_complete =
                message.finish_reason == Some(FinishReason::Stop) && message.tool_calls.is_empty();

            // Should yield if a tool is asking for a follow-up
            should_yield = is_complete
                || message
                    .tool_calls
                    .iter()
                    .any(|call| ToolCatalog::should_yield(&call.name));

            // Process tool calls and update context
            let mut tool_call_records = self
                .execute_tool_calls(&message.tool_calls, &tool_context)
                .await?;

            // Update context from conversation after response / tool-call hooks run
            if let Some(updated_context) = &self.conversation.context {
                context = updated_context.clone();
            }

            self.error_tracker.adjust_record(&tool_call_records);
            let allowed_max_attempts = self.error_tracker.limit();
            for (_, result) in tool_call_records.iter_mut() {
                if result.is_error() {
                    let attempts_left = self.error_tracker.remaining_attempts(&result.name);
                    // Add attempt information to the error message so the agent can reflect on it.
                    let context = serde_json::json!({
                        "attempts_left": attempts_left,
                        "allowed_max_attempts": allowed_max_attempts,
                    });
                    let text = TemplateEngine::default()
                        .render("forge-tool-retry-message.md", &context)?;
                    let message = Element::new("retry").text(text);

                    result.output.combine_mut(ToolOutput::text(message));
                }
            }

            context = context.append_message(
                message.content.clone(),
                message.thought_signature.clone(),
                message.reasoning.clone(),
                message.reasoning_details.clone(),
                message.usage,
                tool_call_records,
                message.phase,
            );

            if self.error_tracker.limit_reached() {
                self.send(ChatResponse::Interrupt {
                    reason: InterruptionReason::MaxToolFailurePerTurnLimitReached {
                        limit: *self.error_tracker.limit() as u64,
                        errors: self.error_tracker.errors().clone(),
                    },
                })
                .await?;
                // Should yield if too many errors are produced
                should_yield = true;
            }

            // Update context in the conversation
            context = SetModel::new(model_id.clone()).transform(context);
            self.conversation.context = Some(context.clone());
            self.services.update(self.conversation.clone()).await?;
            request_count += 1;

            if !should_yield && let Some(max_request_allowed) = max_requests_per_turn {
                // Check if agent has reached the maximum request per turn limit
                if request_count >= max_request_allowed {
                    // Log warning - important for understanding conversation interruptions
                    warn!(
                        agent_id = %self.agent.id,
                        model_id = %model_id,
                        request_count,
                        max_request_allowed,
                        "Agent has reached the maximum request per turn limit"
                    );
                    // raise an interrupt event to notify the UI
                    self.send(ChatResponse::Interrupt {
                        reason: InterruptionReason::MaxRequestPerTurnLimitReached {
                            limit: max_request_allowed as u64,
                        },
                    })
                    .await?;
                    // force completion
                    should_yield = true;
                }
            }

            // Update metrics in conversation
            tool_context.with_metrics(|metrics| {
                self.conversation.metrics = metrics.clone();
            })?;

            // If completing (should_yield is due), fire End hook and check if
            // it adds messages
            if should_yield {
                let end_count_before = self.conversation.len();

                // Legacy End event (kept for internal handlers)
                self.hook
                    .handle(
                        &LifecycleEvent::End(EventData::with_context(
                            self.agent.clone(),
                            model_id.clone(),
                            session_id.clone(),
                            transcript_path.clone(),
                            cwd.clone(),
                            EndPayload,
                        )),
                        &mut self.conversation,
                    )
                    .await?;

                // Claude Code Stop event
                self.conversation.reset_hook_result();
                let stop_payload = StopPayload {
                    stop_hook_active: false,
                    last_assistant_message: last_assistant_content.clone(),
                };
                self.hook
                    .handle(
                        &LifecycleEvent::Stop(EventData::with_context(
                            self.agent.clone(),
                            model_id.clone(),
                            session_id.clone(),
                            transcript_path.clone(),
                            cwd.clone(),
                            stop_payload,
                        )),
                        &mut self.conversation,
                    )
                    .await?;

                let stop_hook_result = std::mem::take(&mut self.conversation.hook_result);

                // Inject additional_contexts as <system_reminder> messages
                if !stop_hook_result.additional_contexts.is_empty()
                    && let Some(ctx) = self.conversation.context.as_mut()
                {
                    for extra in &stop_hook_result.additional_contexts {
                        let wrapped = Element::new("system_reminder").text(extra);
                        ctx.messages.push(
                            ContextMessage::system_reminder(wrapped, Some(model_id.clone())).into(),
                        );
                    }
                }

                self.services.update(self.conversation.clone()).await?;

                // If a Stop hook set prevent_continuation=true OR legacy End hook
                // added messages, re-enter the loop rather than yielding. This
                // mirrors the legacy "End hook added messages" check.
                let legacy_added_messages = self.conversation.len() > end_count_before;
                if legacy_added_messages || stop_hook_result.prevent_continuation {
                    if let Some(updated_context) = &self.conversation.context {
                        context = updated_context.clone();
                    }
                    should_yield = false;
                }
            }
        }

        self.services.update(self.conversation.clone()).await?;

        // Signal Task Completion
        if is_complete {
            self.send(ChatResponse::TaskComplete).await?;
        }

        // Fire SessionEnd (Claude Code plugin event) right before we
        // yield control back to the caller. We ignore hook_result here
        // because the session is ending — any plugin mutations would be
        // lost on the next run.
        self.conversation.reset_hook_result();
        let session_end_payload = SessionEndPayload { reason: SessionEndReason::Other };
        let session_end_event = LifecycleEvent::SessionEnd(EventData::with_context(
            self.agent.clone(),
            model_id.clone(),
            session_id.clone(),
            transcript_path.clone(),
            cwd.clone(),
            session_end_payload,
        ));
        self.hook
            .handle(&session_end_event, &mut self.conversation)
            .await?;

        Ok(())
    }

    fn get_model(&self) -> ModelId {
        self.agent.model.clone()
    }
}
