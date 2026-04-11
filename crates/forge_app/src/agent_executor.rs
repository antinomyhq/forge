use std::sync::Arc;

use anyhow::Context;
use convert_case::{Case, Casing};
use forge_domain::{
    Agent, AgentId, ChatRequest, ChatResponse, ChatResponseContent, Conversation, ConversationId,
    Event, EventData, EventHandle, SubagentStartPayload, SubagentStopPayload, TitleFormat,
    ToolCallContext, ToolDefinition, ToolName, ToolOutput,
};
use forge_template::Element;
use futures::StreamExt;
use tokio::sync::RwLock;

use crate::error::Error;
use crate::hooks::PluginHookHandler;
use crate::{AgentRegistry, ConversationService, EnvironmentInfra, Services};
#[derive(Clone)]
pub struct AgentExecutor<S> {
    services: Arc<S>,
    pub tool_agents: Arc<RwLock<Option<Vec<ToolDefinition>>>>,
    /// Shared plugin hook dispatcher used for the
    /// `SubagentStart` / `SubagentStop` fire sites inside
    /// [`AgentExecutor::execute`]. Reuses the handler constructed by
    /// `ForgeApp::chat` so the once-fired tracking stays consistent
    /// with the rest of the lifecycle chain.
    plugin_handler: PluginHookHandler<S>,
}

impl<S: Services + EnvironmentInfra<Config = forge_config::ForgeConfig>> AgentExecutor<S> {
    pub fn new(services: Arc<S>, plugin_handler: PluginHookHandler<S>) -> Self {
        Self {
            services,
            tool_agents: Arc::new(RwLock::new(None)),
            plugin_handler,
        }
    }

    /// Returns a list of tool definitions for all available agents.
    pub async fn agent_definitions(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        if let Some(tool_agents) = self.tool_agents.read().await.clone() {
            return Ok(tool_agents);
        }
        let agents = self.services.get_agents().await?;
        let tools: Vec<ToolDefinition> = agents.into_iter().map(Into::into).collect();
        *self.tool_agents.write().await = Some(tools.clone());
        Ok(tools)
    }

    /// Executes an agent tool call by creating a new chat request for the
    /// Executes an agent tool call by creating a new chat request for the
    /// specified agent. If conversation_id is provided, the agent will reuse
    /// that conversation, maintaining context across invocations. Otherwise,
    /// a new conversation is created.
    pub async fn execute(
        &self,
        agent_id: AgentId,
        task: String,
        ctx: &ToolCallContext,
        conversation_id: Option<ConversationId>,
    ) -> anyhow::Result<ToolOutput> {
        ctx.send_tool_input(
            TitleFormat::debug(format!(
                "{} [Agent]",
                agent_id.as_str().to_case(Case::UpperSnake)
            ))
            .sub_title(task.as_str()),
        )
        .await?;

        // Reuse existing conversation if provided, otherwise create a new one
        let mut conversation = if let Some(conversation_id) = conversation_id {
            self.services
                .conversation_service()
                .find_conversation(&conversation_id)
                .await?
                .ok_or(Error::ConversationNotFound { id: conversation_id })?
        } else {
            // Create context with agent initiator since it's spawned by a parent agent
            // This is crucial for GitHub Copilot billing optimization
            let context = forge_domain::Context::default().initiator("agent".to_string());
            let conversation = Conversation::generate()
                .title(task.clone())
                .context(context.clone());
            self.services
                .conversation_service()
                .upsert_conversation(conversation.clone())
                .await?;
            conversation
        };

        // ---- SubagentStart fire site ----
        //
        // Generate a stable subagent UUID for this execution. Using
        // `ConversationId::generate()` keeps the id a uuid v4 string
        // without pulling `uuid` into `forge_app`'s direct
        // dependencies.
        let subagent_id: String = ConversationId::generate().into_string();
        let agent_type: String = agent_id.as_str().to_string();

        // Resolve the child agent for the event context. Fall back to
        // a minimal Agent built from the id (matching the
        // `lifecycle_fires::fire_setup_hook` fallback pattern) so the
        // fire site never panics when the registry lookup fails.
        let env = self.services.get_environment();
        let session_id = conversation.id.into_string();
        let transcript_path = env.transcript_path(&session_id);
        let cwd = env.cwd.clone();

        let child_agent = match self.services.get_agent(&agent_id).await {
            Ok(Some(a)) => a,
            _ => {
                // Fall back to the first registered agent so we have a
                // real ModelId on the event, mirroring the
                // `fire_setup_hook` fallback. If the registry is
                // empty, build a minimal placeholder — the ModelId is unused
                // by the plugin dispatcher for `SubagentStart` /
                // `SubagentStop` (the matcher filters on agent_type).
                let agents = self.services.get_agents().await.ok().unwrap_or_default();
                match agents.into_iter().next() {
                    Some(a) => a,
                    None => Agent::new(
                        agent_id.clone(),
                        forge_domain::ProviderId::FORGE,
                        forge_domain::ModelId::new(""),
                    ),
                }
            }
        };
        let model_id = child_agent.model.clone();

        let start_payload = SubagentStartPayload {
            agent_id: subagent_id.clone(),
            agent_type: agent_type.clone(),
        };
        let start_event_data = EventData::with_context(
            child_agent.clone(),
            model_id.clone(),
            session_id.clone(),
            transcript_path.clone(),
            cwd.clone(),
            start_payload,
        );

        // Reset hook_result on the subagent's conversation before
        // dispatching so we observe only this event's aggregated
        // output.
        conversation.reset_hook_result();
        if let Err(err) =
            <PluginHookHandler<S> as EventHandle<EventData<SubagentStartPayload>>>::handle(
                &self.plugin_handler,
                &start_event_data,
                &mut conversation,
            )
            .await
        {
            tracing::warn!(error = ?err, "SubagentStart hook dispatch failed");
        }

        // Consume blocking_error: if a plugin blocked the subagent
        // from starting, propagate it as a non-fatal tool output
        // without calling ForgeApp::chat at all.
        if let Some(blocking) = conversation.hook_result.blocking_error.take() {
            return Ok(ToolOutput::text(format!(
                "Subagent '{agent_type}' blocked by plugin hook: {msg}",
                msg = blocking.message
            )));
        }

        // Consume additional_contexts emitted by SubagentStart hooks.
        //
        // TODO(subagent-context-injection): This uses the fallback
        // simplification — prepend each additional context wrapped in
        // `<system_reminder>` tags to the `task` string so the inner
        // orchestrator receives them via the `UserPromptSubmit` event.
        // A cleaner refactor (injecting
        // `ContextMessage::system_reminder` into the subagent's
        // `Conversation.context` before `upsert_conversation`) is
        // pending; the fallback keeps things simple and robust against
        // the `SystemPrompt::add_system_message` overwrite that
        // happens inside `ForgeApp::chat`.
        let extra_contexts: Vec<String> = conversation
            .hook_result
            .additional_contexts
            .drain(..)
            .collect();
        let effective_task: String = if extra_contexts.is_empty() {
            task.clone()
        } else {
            let mut buf = String::new();
            for extra in &extra_contexts {
                let wrapped = Element::new("system_reminder").text(extra).render();
                buf.push_str(&wrapped);
                buf.push('\n');
            }
            buf.push_str(&task);
            buf
        };

        // Execute the request through the ForgeApp
        let app = crate::ForgeApp::new(self.services.clone());
        let chat_stream_result = app
            .chat(
                agent_id.clone(),
                ChatRequest::new(Event::new(effective_task.clone()), conversation.id),
            )
            .await;

        // Helper closure to fire SubagentStop from both the success
        // and failure paths. Takes the last assistant message so
        // successful runs populate it and error paths leave it
        // `None`.
        //
        // The closure captures the context values by cloning so the
        // happy path can reuse them when building the final
        // `ToolOutput` without moving anything prematurely.
        async fn fire_subagent_stop<S: Services>(
            handler: &PluginHookHandler<S>,
            conversation: &mut Conversation,
            child_agent: Agent,
            model_id: forge_domain::ModelId,
            session_id: String,
            transcript_path: std::path::PathBuf,
            cwd: std::path::PathBuf,
            subagent_id: String,
            agent_type: String,
            last_assistant_message: Option<String>,
        ) {
            let stop_payload = SubagentStopPayload {
                agent_id: subagent_id,
                agent_type,
                agent_transcript_path: transcript_path.clone(),
                // Can be wired from hook-driven
                // `prevent_continuation` output in the future.
                stop_hook_active: false,
                last_assistant_message,
            };
            let stop_event_data = EventData::with_context(
                child_agent,
                model_id,
                session_id,
                transcript_path,
                cwd,
                stop_payload,
            );
            conversation.reset_hook_result();
            if let Err(err) = <PluginHookHandler<S> as EventHandle<
                EventData<SubagentStopPayload>,
            >>::handle(handler, &stop_event_data, conversation)
            .await
            {
                tracing::warn!(error = ?err, "SubagentStop hook dispatch failed");
            }
            // Drain and discard blocking_error — SubagentStop is
            // observability-only per Claude Code semantics.
            let _ = conversation.hook_result.blocking_error.take();
        }

        // If `ForgeApp::chat` itself failed (e.g. agent not found,
        // auth error), fire SubagentStop with no last message and
        // propagate the error.
        let mut response_stream = match chat_stream_result {
            Ok(stream) => stream,
            Err(err) => {
                fire_subagent_stop(
                    &self.plugin_handler,
                    &mut conversation,
                    child_agent.clone(),
                    model_id.clone(),
                    session_id.clone(),
                    transcript_path.clone(),
                    cwd.clone(),
                    subagent_id.clone(),
                    agent_type.clone(),
                    None,
                )
                .await;
                return Err(err);
            }
        };

        // Collect responses from the agent. Errors emitted mid-stream
        // still need to fire SubagentStop, so we unwrap the inner
        // match into a local result variable rather than using `?`.
        let mut output = String::new();
        let drain_result: anyhow::Result<()> = async {
            while let Some(message) = response_stream.next().await {
                let message = message?;
                if matches!(
                    &message,
                    ChatResponse::ToolCallStart { .. } | ChatResponse::ToolCallEnd(_)
                ) {
                    output.clear();
                }
                match message {
                    ChatResponse::TaskMessage { ref content } => match content {
                        ChatResponseContent::ToolInput(_) => ctx.send(message).await?,
                        ChatResponseContent::ToolOutput(_) => {}
                        ChatResponseContent::Markdown { text, partial } => {
                            if *partial {
                                output.push_str(text);
                            } else {
                                output = text.to_string();
                            }
                        }
                    },
                    ChatResponse::TaskReasoning { .. } => {}
                    ChatResponse::TaskComplete => {}
                    ChatResponse::ToolCallStart { .. } => ctx.send(message).await?,
                    ChatResponse::ToolCallEnd(_) => ctx.send(message).await?,
                    ChatResponse::RetryAttempt { .. } => ctx.send(message).await?,
                    ChatResponse::Interrupt { reason } => {
                        return Err(anyhow::Error::from(Error::AgentToolInterrupted(reason)))
                            .context(format!(
                                "Tool call to '{}' failed.\n\
                                 Note: This is an AGENTIC tool (powered by an LLM), not a traditional function.\n\
                                 The failure occurred because the underlying LLM did not behave as expected.\n\
                                 This is typically caused by model limitations, prompt issues, or reaching safety limits.",
                                agent_id.as_str()
                            ));
                    }
                }
            }
            Ok(())
        }
        .await;

        // Fire SubagentStop regardless of success / failure so plugins
        // get a paired Start + Stop even when the subagent blew up
        // mid-stream.
        let last_assistant_message = if output.is_empty() {
            None
        } else {
            Some(output.clone())
        };
        fire_subagent_stop(
            &self.plugin_handler,
            &mut conversation,
            child_agent,
            model_id,
            session_id,
            transcript_path,
            cwd,
            subagent_id,
            agent_type,
            last_assistant_message,
        )
        .await;

        // Now propagate any error we captured while draining the
        // stream.
        drain_result?;

        if !output.is_empty() {
            // Create tool output
            Ok(ToolOutput::ai(
                conversation.id,
                Element::new("task_completed")
                    .attr("task", &task)
                    .append(Element::new("output").text(output)),
            ))
        } else {
            Err(Error::EmptyToolResponse.into())
        }
    }

    pub async fn contains_tool(&self, tool_name: &ToolName) -> anyhow::Result<bool> {
        let agent_tools = self.agent_definitions().await?;
        Ok(agent_tools.iter().any(|tool| tool.name == *tool_name))
    }
}

// ---- Fire-site payload construction tests ----
//
// TODO(full-executor-tests): These are construction-level unit
// tests for the `SubagentStart` / `SubagentStop` payloads built inside
// `AgentExecutor::execute`. A full integration harness that mocks
// `Services` (including `ConversationService`, `AgentRegistry`,
// `hook_config_loader`, `hook_executor`) is not yet available —
// the full end-to-end happy-path, blocking-error, and context-injection
// flows will be covered once a shared `MockServices` test kit lands.
// Until then, these tests document the field wiring that
// `AgentExecutor::execute` performs and the existing dispatcher tests in
// `crates/forge_app/src/hooks/plugin.rs` cover the matcher / once /
// aggregation semantics for `SubagentStart` and `SubagentStop`.
#[cfg(test)]
mod tests {
    use forge_domain::{
        Agent, AgentId, ConversationId, EventData, ModelId, ProviderId, SubagentStartPayload,
        SubagentStopPayload,
    };
    use pretty_assertions::assert_eq;

    #[test]
    fn test_subagent_start_payload_field_wiring_from_agent_id() {
        // Given: an incoming `AgentId` (e.g. "muse") and a freshly
        // generated subagent uuid.
        let agent_id = AgentId::new("muse");
        let subagent_id = ConversationId::generate().into_string();
        let agent_type = agent_id.as_str().to_string();

        // When: the fire site builds the payload the same way
        // `AgentExecutor::execute` does.
        let payload = SubagentStartPayload {
            agent_id: subagent_id.clone(),
            agent_type: agent_type.clone(),
        };

        // Then: both fields mirror the inputs.
        assert_eq!(payload.agent_type, "muse");
        assert_eq!(payload.agent_id, subagent_id);
        // Uuid v4 canonical string is 36 chars (8-4-4-4-12 plus hyphens).
        assert_eq!(payload.agent_id.len(), 36);
    }

    #[test]
    fn test_subagent_stop_payload_field_wiring_happy_path() {
        // Given: a completed subagent run with a non-empty final
        // assistant message.
        let agent_id = AgentId::new("sage");
        let subagent_id = ConversationId::generate().into_string();
        let agent_type = agent_id.as_str().to_string();
        let transcript_path = std::path::PathBuf::from("/tmp/forge/sessions/abc.jsonl");
        let final_output = "All done!".to_string();

        // When: the fire site builds SubagentStop with
        // `last_assistant_message = Some(final_output)` because the
        // happy-path drain produced output.
        let payload = SubagentStopPayload {
            agent_id: subagent_id.clone(),
            agent_type: agent_type.clone(),
            agent_transcript_path: transcript_path.clone(),
            stop_hook_active: false,
            last_assistant_message: Some(final_output.clone()),
        };

        // Then: every field reflects the subagent run.
        assert_eq!(payload.agent_id, subagent_id);
        assert_eq!(payload.agent_type, "sage");
        assert_eq!(payload.agent_transcript_path, transcript_path);
        assert!(!payload.stop_hook_active);
        assert_eq!(payload.last_assistant_message, Some(final_output));
    }

    #[test]
    fn test_subagent_stop_payload_last_assistant_message_is_none_on_empty_output() {
        // Given: a subagent run that emitted no final text (e.g.
        // interrupted mid-stream, or chat error). The fire site maps
        // an empty `output` string to `None` so observability plugins
        // can distinguish "no message" from "empty message".
        let agent_id = AgentId::new("forge");
        let subagent_id = ConversationId::generate().into_string();
        let agent_type = agent_id.as_str().to_string();
        let transcript_path = std::path::PathBuf::from("/tmp/forge/sessions/xyz.jsonl");

        let output = String::new();
        let last_assistant_message = if output.is_empty() {
            None
        } else {
            Some(output.clone())
        };

        let payload = SubagentStopPayload {
            agent_id: subagent_id,
            agent_type,
            agent_transcript_path: transcript_path,
            stop_hook_active: false,
            last_assistant_message,
        };

        assert_eq!(payload.last_assistant_message, None);
    }

    #[test]
    fn test_event_data_with_context_threads_subagent_payload() {
        // Given: the fire site resolves the child `Agent` and a
        // `ModelId` before building `EventData`.
        let agent_id = AgentId::new("muse");
        let agent = Agent::new(agent_id.clone(), ProviderId::FORGE, ModelId::new("gpt-5"));
        let subagent_id = ConversationId::generate().into_string();
        let transcript_path = std::path::PathBuf::from("/tmp/forge/sessions/inner.jsonl");
        let cwd = std::path::PathBuf::from("/repo");

        // When: the fire site wraps `SubagentStartPayload` in an
        // `EventData` carrying the subagent's own session id.
        let payload = SubagentStartPayload {
            agent_id: subagent_id.clone(),
            agent_type: agent_id.as_str().to_string(),
        };
        let event_data: EventData<SubagentStartPayload> = EventData::with_context(
            agent.clone(),
            agent.model.clone(),
            "subagent-session-id".to_string(),
            transcript_path.clone(),
            cwd.clone(),
            payload,
        );

        // Then: `EventData` carries the subagent's session id and
        // transcript path, and the inner payload preserves both
        // agent_id (UUID) and agent_type.
        assert_eq!(event_data.session_id, "subagent-session-id");
        assert_eq!(event_data.transcript_path, transcript_path);
        assert_eq!(event_data.cwd, cwd);
        assert_eq!(event_data.payload.agent_id, subagent_id);
        assert_eq!(event_data.payload.agent_type, "muse");
        // The EventData carries the child agent, so the wire-level
        // HookInputBase.agent_id defaults to the child agent's id —
        // Task 7's subagent UUID override is pending (see
        // `TODO(subagent-threading)` in orch.rs).
        assert_eq!(event_data.agent.id.as_str(), "muse");
    }
}
