use std::sync::Arc;

use anyhow::Result;
use chrono::Local;
use forge_config::ForgeConfig;
use forge_domain::*;
use forge_stream::MpscStream;

use crate::apply_tunable_parameters::ApplyTunableParameters;
use crate::changed_files::ChangedFiles;
use crate::dto::ToolsOverview;
use crate::hooks::{
    CompactionHandler, DoomLoopDetector, PendingTodosHandler, PluginHookHandler,
    SkillCacheInvalidator, SkillListingHandler, TitleGenerationHandler, TracingHandler,
};
use crate::init_conversation_metrics::InitConversationMetrics;
use crate::orch::Orchestrator;
use crate::services::{AgentRegistry, CustomInstructionsService, ProviderAuthService};
use crate::set_conversation_id::SetConversationId;
use crate::system_prompt::SystemPrompt;
use crate::tool_registry::ToolRegistry;
use crate::tool_resolver::ToolResolver;
use crate::user_prompt::UserPromptGenerator;
use crate::{
    AgentExt, AgentProviderResolver, ConversationService, EnvironmentInfra, FileDiscoveryService,
    ProviderService, Services,
};

/// Builds a [`TemplateConfig`] from a [`ForgeConfig`].
///
/// Converts the configuration-layer field names into the domain-layer struct
/// expected by [`SystemContext`] for tool description template rendering.
pub(crate) fn build_template_config(config: &ForgeConfig) -> forge_domain::TemplateConfig {
    forge_domain::TemplateConfig {
        max_read_size: config.max_read_lines as usize,
        max_line_length: config.max_line_chars,
        max_image_size: config.max_image_size_bytes as usize,
        stdout_max_prefix_length: config.max_stdout_prefix_lines,
        stdout_max_suffix_length: config.max_stdout_suffix_lines,
        stdout_max_line_length: config.max_stdout_line_chars,
    }
}

/// ForgeApp handles the core chat functionality by orchestrating various
/// services. It encapsulates the complex logic previously contained in the
/// ForgeAPI chat method.
pub struct ForgeApp<S> {
    services: Arc<S>,
    tool_registry: ToolRegistry<S>,
    /// Shared plugin hook dispatcher. Created once in
    /// [`ForgeApp::new`] and reused by both the `ToolRegistry`
    /// (`AgentExecutor::execute` fire sites) and
    /// [`ForgeApp::chat`] (main Hook chain builder). Reusing the
    /// same handle keeps `once_fired` tracking and any future
    /// per-handler state consistent across the whole pipeline.
    plugin_handler: PluginHookHandler<S>,
}

impl<S: Services + EnvironmentInfra<Config = forge_config::ForgeConfig>> ForgeApp<S> {
    /// Creates a new ForgeApp instance with the provided services.
    pub fn new(services: Arc<S>) -> Self {
        // Shared plugin hook dispatcher — passed into both `ToolRegistry`
        // (so `AgentExecutor` can fire `SubagentStart` / `SubagentStop`
        // from inside `execute`) and later reused verbatim by
        // `ForgeApp::chat` when building the `Hook` chain. Constructing
        // the handler at `ForgeApp::new` time keeps the once-fired
        // tracking anchored to a single instance per chat pipeline.
        let plugin_handler = PluginHookHandler::new(services.clone());
        Self {
            tool_registry: ToolRegistry::new(services.clone(), plugin_handler.clone()),
            plugin_handler,
            services,
        }
    }

    /// Executes a chat request and returns a stream of responses.
    /// This method contains the core chat logic extracted from ForgeAPI.
    pub async fn chat(
        &self,
        agent_id: AgentId,
        chat: ChatRequest,
    ) -> Result<MpscStream<Result<ChatResponse, anyhow::Error>>> {
        let services = self.services.clone();

        // Get the conversation for the chat request
        let conversation = services
            .find_conversation(&chat.conversation_id)
            .await?
            .ok_or_else(|| forge_domain::Error::ConversationNotFound(chat.conversation_id))?;

        // Discover files using the discovery service
        let forge_config = self.services.get_config()?;
        let environment = services.get_environment();

        let files = services.list_current_directory().await?;

        // Load instructions with full classification metadata so we
        // can fire one `InstructionsLoaded` hook per discovered
        // AGENTS.md file. The system prompt builder still only needs
        // the raw text, so we project the `content` field back into a
        // `Vec<String>` for `custom_instructions`.
        let loaded_instructions = services.get_custom_instructions_detailed().await;
        let custom_instructions: Vec<String> = loaded_instructions
            .iter()
            .map(|loaded| loaded.content.clone())
            .collect();

        // Fire the InstructionsLoaded hook once per loaded file. Each
        // fire is observability-only — hook dispatch errors are
        // logged inside `fire_instructions_loaded_hook` and never
        // propagated to the chat pipeline.
        for loaded in &loaded_instructions {
            crate::lifecycle_fires::fire_instructions_loaded_hook(services.clone(), loaded.clone())
                .await;
        }

        // Prepare agents with user configuration
        let agent_provider_resolver = AgentProviderResolver::new(services.clone());

        // Get agent and apply workflow config
        let agent = self
            .services
            .get_agent(&agent_id)
            .await?
            .ok_or(crate::Error::AgentNotFound(agent_id.clone()))?
            .apply_config(&forge_config)
            .set_compact_model_if_none();

        let agent_provider = agent_provider_resolver
            .get_provider(Some(agent.id.clone()))
            .await?;
        let agent_provider = self
            .services
            .provider_auth_service()
            .refresh_provider_credential(agent_provider)
            .await?;

        let models = services.models(agent_provider).await?;

        // Get system and mcp tool definitions and resolve them for the agent
        let all_tool_definitions = self.tool_registry.list().await?;
        let tool_resolver = ToolResolver::new(all_tool_definitions);
        let tool_definitions: Vec<ToolDefinition> =
            tool_resolver.resolve(&agent).into_iter().cloned().collect();
        let max_tool_failure_per_turn = agent.max_tool_failure_per_turn.unwrap_or(3);

        let current_time = Local::now();

        // Insert system prompt
        let conversation =
            SystemPrompt::new(self.services.clone(), environment.clone(), agent.clone())
                .custom_instructions(custom_instructions.clone())
                .tool_definitions(tool_definitions.clone())
                .models(models.clone())
                .files(files.clone())
                .max_extensions(forge_config.max_extensions)
                .template_config(build_template_config(&forge_config))
                .add_system_message(conversation)
                .await?;

        // Insert user prompt
        // Capture the raw user prompt text (pre-templating) so the
        // UserPromptSubmit hook payload can be populated. The
        // orchestrator fires UserPromptSubmit on the first iteration of
        // its main loop.
        let raw_user_prompt: Option<String> = chat
            .event
            .value
            .as_ref()
            .and_then(|v| v.as_user_prompt().map(|p| p.as_str().to_string()));

        let conversation = UserPromptGenerator::new(
            self.services.clone(),
            agent.clone(),
            chat.event.clone(),
            current_time,
        )
        .add_user_prompt(conversation)
        .await?;

        // Detect and render externally changed files notification
        let conversation = ChangedFiles::new(services.clone(), agent.clone())
            .update_file_stats(conversation)
            .await;

        let conversation = InitConversationMetrics::new(current_time).apply(conversation);
        let conversation = ApplyTunableParameters::new(agent.clone(), tool_definitions.clone())
            .apply(conversation);
        let conversation = SetConversationId.apply(conversation);

        // Create the orchestrator with all necessary dependencies
        let tracing_handler = TracingHandler::new();
        let title_handler = TitleGenerationHandler::new(services.clone());

        // Build the on_end hook. `PendingTodosHandler` now runs on the
        // Claude-Code `Stop` event instead (see `on_stop_hook` below).
        let on_end_hook = tracing_handler.clone().and(title_handler.clone());

        // Determine context window for skill listing budget. Falls back to the
        // handler's default (~200k) when the active model doesn't advertise a
        // context length.
        let skill_listing_handler = {
            let mut h = SkillListingHandler::new(services.clone());
            if let Some(ctx_len) = models
                .iter()
                .find(|m| m.id == agent.model)
                .and_then(|m| m.context_length)
            {
                h = h.context_tokens(ctx_len);
            }
            h
        };
        let skill_cache_invalidator = SkillCacheInvalidator::new(services.clone());

        // Shared plugin hook dispatcher used for every Claude-Code-compatible
        // lifecycle event.
        //
        // Reuse the handle constructed in `ForgeApp::new` so
        // the `AgentExecutor` fire sites for `SubagentStart` /
        // `SubagentStop` share the same `once_fired` tracking with the
        // rest of the Hook chain.
        let plugin_handler = self.plugin_handler.clone();

        // Build the on_stop hook chain, conditionally adding
        // `PendingTodosHandler` based on config. `PendingTodosHandler`
        // runs on Claude Code's `Stop` event (not the legacy `End`
        // event). Both branches must unify to the same
        // `Box<dyn EventHandle<_>>` type — `.and(NoOpHandler)` in the
        // else branch gives us that without changing behaviour.
        let on_stop_hook = if forge_config.verify_todos {
            plugin_handler.clone().and(PendingTodosHandler::new())
        } else {
            plugin_handler.clone().and(NoOpHandler)
        };

        let hook = Hook::default()
            .on_start(tracing_handler.clone().and(title_handler))
            .on_request(
                tracing_handler
                    .clone()
                    .and(DoomLoopDetector::default())
                    .and(skill_listing_handler),
            )
            .on_response(tracing_handler.clone().and(CompactionHandler::new(
                agent.clone(),
                environment.clone(),
                plugin_handler.clone(),
            )))
            .on_toolcall_start(tracing_handler.clone())
            .on_toolcall_end(tracing_handler.clone().and(skill_cache_invalidator))
            .on_end(on_end_hook)
            .on_pre_tool_use(plugin_handler.clone())
            .on_post_tool_use(plugin_handler.clone())
            .on_post_tool_use_failure(plugin_handler.clone())
            .on_user_prompt_submit(plugin_handler.clone())
            .on_session_start(plugin_handler.clone())
            .on_session_end(plugin_handler.clone())
            .on_stop(on_stop_hook)
            .on_stop_failure(plugin_handler.clone())
            .on_pre_compact(plugin_handler.clone())
            .on_post_compact(plugin_handler.clone())
            .on_notification(plugin_handler.clone())
            .on_config_change(plugin_handler.clone())
            .on_setup(plugin_handler.clone())
            .on_instructions_loaded(plugin_handler.clone())
            .on_subagent_start(plugin_handler.clone())
            .on_subagent_stop(plugin_handler.clone())
            .on_permission_request(plugin_handler.clone())
            .on_permission_denied(plugin_handler.clone())
            .on_cwd_changed(plugin_handler.clone())
            .on_file_changed(plugin_handler.clone())
            .on_worktree_create(plugin_handler.clone())
            .on_worktree_remove(plugin_handler.clone())
            .on_elicitation(plugin_handler.clone())
            .on_elicitation_result(plugin_handler);

        let mut orch = Orchestrator::new(
            services.clone(),
            conversation,
            agent,
            self.services.get_config()?,
        )
        .error_tracker(ToolErrorTracker::new(max_tool_failure_per_turn))
        .tool_definitions(tool_definitions)
        .models(models)
        .hook(Arc::new(hook));
        if let Some(prompt) = raw_user_prompt {
            orch = orch.user_prompt(prompt);
        }
        if let Some(queue) = self.services.async_hook_queue() {
            orch = orch.async_hook_queue(queue.clone());
        }

        // Create and return the stream
        let stream = MpscStream::spawn(
            |tx: tokio::sync::mpsc::Sender<Result<ChatResponse, anyhow::Error>>| {
                async move {
                    // Execute dispatch and always save conversation afterwards
                    let mut orch = orch.sender(tx.clone());
                    let dispatch_result = orch.run().await;

                    // Always save conversation using get_conversation()
                    let conversation = orch.get_conversation().clone();
                    let save_result = services.upsert_conversation(conversation).await;

                    // Send any error to the stream (prioritize dispatch error over save error)
                    #[allow(clippy::collapsible_if)]
                    if let Some(err) = dispatch_result.err().or(save_result.err()) {
                        if let Err(e) = tx.send(Err(err)).await {
                            tracing::error!("Failed to send error to stream: {}", e);
                        }
                    }
                }
            },
        );

        Ok(stream)
    }

    /// Compacts the context of the main agent for the given conversation and
    /// persists it. Returns metrics about the compaction (original vs.
    /// compacted tokens and messages).
    pub async fn compact_conversation(
        &self,
        active_agent_id: AgentId,
        conversation_id: &ConversationId,
    ) -> Result<CompactionResult> {
        use crate::compact::Compactor;

        // Get the conversation
        let mut conversation = self
            .services
            .find_conversation(conversation_id)
            .await?
            .ok_or_else(|| forge_domain::Error::ConversationNotFound(*conversation_id))?;

        // Get the context from the conversation
        let context = match conversation.context.as_ref() {
            Some(context) => context.clone(),
            None => {
                // No context to compact, return zero metrics
                return Ok(CompactionResult::new(0, 0, 0, 0));
            }
        };

        // Calculate original metrics
        let original_messages = context.messages.len();
        let original_token_count = *context.token_count();

        let forge_config = self.services.get_config()?;

        // Get agent and apply workflow config
        let agent = self.services.get_agent(&active_agent_id).await?;

        let Some(agent) = agent else {
            return Ok(CompactionResult::new(
                original_token_count,
                0,
                original_messages,
                0,
            ));
        };

        // Get compact config from the agent
        let compact = agent
            .clone()
            .apply_config(&forge_config)
            .set_compact_model_if_none()
            .compact;

        // Apply compaction using the Compactor
        let environment = self.services.get_environment();

        // Fire PreCompact plugin hook. Manual compact
        // uses CompactTrigger::Manual. A blocking hook aborts the
        // compaction with an error.
        let plugin_handler = PluginHookHandler::new(self.services.clone());
        let session_id = conversation.id.into_string();
        let transcript_path = environment.transcript_path(&session_id);
        let cwd = environment.cwd.clone();

        conversation.reset_hook_result();
        let pre_payload =
            PreCompactPayload { trigger: CompactTrigger::Manual, custom_instructions: None };
        let pre_event_data = EventData::with_context(
            agent.clone(),
            agent.model.clone(),
            session_id.clone(),
            transcript_path.clone(),
            cwd.clone(),
            pre_payload,
        );
        <PluginHookHandler<S> as EventHandle<EventData<PreCompactPayload>>>::handle(
            &plugin_handler,
            &pre_event_data,
            &mut conversation,
        )
        .await?;

        let pre_hook_result = std::mem::take(&mut conversation.hook_result);
        if let Some(err) = pre_hook_result.blocking_error {
            return Err(anyhow::anyhow!(
                "Manual compaction blocked by plugin hook: {}",
                err.message
            ));
        }

        let compacted_context = Compactor::new(compact, environment).compact(context, true)?;

        let compacted_messages = compacted_context.messages.len();
        let compacted_tokens = *compacted_context.token_count();

        // Update the conversation with the compacted context
        conversation.context = Some(compacted_context);

        // Fire PostCompact plugin hook. Uses an empty summary for now —
        // real compaction summary extraction is a follow-up.
        conversation.reset_hook_result();
        let post_payload = PostCompactPayload {
            trigger: CompactTrigger::Manual,
            compact_summary: String::new(),
        };
        let post_event_data = EventData::with_context(
            agent.clone(),
            agent.model.clone(),
            session_id,
            transcript_path,
            cwd,
            post_payload,
        );
        <PluginHookHandler<S> as EventHandle<EventData<PostCompactPayload>>>::handle(
            &plugin_handler,
            &post_event_data,
            &mut conversation,
        )
        .await?;
        // Drain hook_result — PostCompact extras are not consumed
        // on this path.
        let _ = std::mem::take(&mut conversation.hook_result);

        // Save the updated conversation
        self.services.upsert_conversation(conversation).await?;

        Ok(CompactionResult::new(
            original_token_count,
            compacted_tokens,
            original_messages,
            compacted_messages,
        ))
    }

    pub async fn list_tools(&self) -> Result<ToolsOverview> {
        self.tool_registry.tools_overview().await
    }

    /// Gets available models for the default provider with automatic credential
    /// refresh.
    pub async fn get_models(&self) -> Result<Vec<Model>> {
        let agent_provider_resolver = AgentProviderResolver::new(self.services.clone());
        let provider = agent_provider_resolver.get_provider(None).await?;
        let provider = self
            .services
            .provider_auth_service()
            .refresh_provider_credential(provider)
            .await?;

        self.services.models(provider).await
    }

    /// Gets available models from all configured providers concurrently.
    ///
    /// Returns a list of `ProviderModels` for each configured provider that
    /// successfully returned models. If every configured provider fails (e.g.
    /// due to an invalid API key), the first error encountered is returned so
    /// the caller receives the real underlying cause rather than an empty list.
    pub async fn get_all_provider_models(&self) -> Result<Vec<ProviderModels>> {
        let all_providers = self.services.get_all_providers().await?;

        // Build one future per configured provider, preserving the error on failure.
        let futures: Vec<_> = all_providers
            .into_iter()
            .filter_map(|any_provider| any_provider.into_configured())
            .map(|provider| {
                let provider_id = provider.id.clone();
                let services = self.services.clone();
                async move {
                    let result: Result<ProviderModels> = async {
                        let refreshed = services
                            .provider_auth_service()
                            .refresh_provider_credential(provider)
                            .await?;
                        let models = services.models(refreshed).await?;
                        Ok(ProviderModels { provider_id, models })
                    }
                    .await;
                    result
                }
            })
            .collect();

        // Execute all provider fetches concurrently.
        futures::future::join_all(futures)
            .await
            .into_iter()
            .collect::<anyhow::Result<Vec<_>>>()
    }
}
