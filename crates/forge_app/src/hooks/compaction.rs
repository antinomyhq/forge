use async_trait::async_trait;
use forge_domain::{
    Agent, CompactTrigger, Conversation, Environment, EventData, EventHandle, LifecycleEvent,
    PostCompactPayload, PreCompactPayload, ResponsePayload,
};
use tracing::{debug, info};

use crate::compact::Compactor;
use crate::hooks::plugin::PluginHookHandler;
use crate::services::Services;

/// Hook handler that performs context compaction when needed
///
/// This handler checks if the conversation context has grown too large
/// and compacts it according to the agent's compaction configuration.
/// The handler mutates the conversation's context in-place if compaction
/// is triggered.
///
/// The `plugin_handler` field fires `PreCompact` and `PostCompact`
/// plugin hook events around the actual compaction call.
pub struct CompactionHandler<S> {
    agent: Agent,
    environment: Environment,
    plugin_handler: PluginHookHandler<S>,
}

impl<S> Clone for CompactionHandler<S> {
    fn clone(&self) -> Self {
        Self {
            agent: self.agent.clone(),
            environment: self.environment.clone(),
            plugin_handler: self.plugin_handler.clone(),
        }
    }
}

impl<S> CompactionHandler<S> {
    /// Creates a new compaction handler
    ///
    /// # Arguments
    /// * `agent` - The agent configuration containing compaction settings
    /// * `environment` - The environment configuration
    /// * `plugin_handler` - Shared plugin hook dispatcher used to fire
    ///   `PreCompact` / `PostCompact` events
    pub fn new(
        agent: Agent,
        environment: Environment,
        plugin_handler: PluginHookHandler<S>,
    ) -> Self {
        Self { agent, environment, plugin_handler }
    }
}

#[async_trait]
impl<S> EventHandle<EventData<ResponsePayload>> for CompactionHandler<S>
where
    S: Services,
{
    async fn handle(
        &self,
        _event: &EventData<ResponsePayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        if let Some(context) = &conversation.context {
            let token_count = context.token_count();
            if self.agent.compact.should_compact(context, *token_count) {
                info!(agent_id = %self.agent.id, "Compaction triggered by hook");

                // Snapshot the current context before any hook fire so we
                // can pass it to Compactor::compact without holding an
                // immutable borrow of `conversation` across hook calls.
                let context_snapshot = context.clone();

                // Resolve plugin-hook context for this auto-compact cycle.
                let session_id = conversation.id.into_string();
                let transcript_path = self.environment.transcript_path(&session_id);
                let cwd = self.environment.cwd.clone();

                // Fire PreCompact — plugin hooks can block the compaction
                // via blocking_error.
                conversation.reset_hook_result();
                let pre_payload =
                    PreCompactPayload { trigger: CompactTrigger::Auto, custom_instructions: None };
                let pre_event = LifecycleEvent::PreCompact(EventData::with_context(
                    self.agent.clone(),
                    self.agent.model.clone(),
                    session_id.clone(),
                    transcript_path.clone(),
                    cwd.clone(),
                    pre_payload,
                ));
                // LifecycleEvent wraps the EventData — dispatch via the
                // typed EventHandle impl on PluginHookHandler. We extract
                // the inner data to avoid going through Hook.
                if let LifecycleEvent::PreCompact(pre_event_data) = &pre_event {
                    <PluginHookHandler<S> as EventHandle<EventData<PreCompactPayload>>>::handle(
                        &self.plugin_handler,
                        pre_event_data,
                        conversation,
                    )
                    .await?;
                }

                let pre_hook_result = std::mem::take(&mut conversation.hook_result);
                if let Some(err) = pre_hook_result.blocking_error {
                    info!(
                        agent_id = %self.agent.id,
                        error = %err.message,
                        "PreCompact hook blocked compaction"
                    );
                    return Ok(());
                }

                // Perform the actual compaction.
                let compacted =
                    Compactor::new(self.agent.compact.clone(), self.environment.clone())
                        .compact(context_snapshot, false)?;
                conversation.context = Some(compacted);

                // Fire PostCompact. Uses an empty summary — a richer
                // compaction summary extraction can be added later.
                conversation.reset_hook_result();
                let post_payload = PostCompactPayload {
                    trigger: CompactTrigger::Auto,
                    compact_summary: String::new(),
                };
                let post_event = LifecycleEvent::PostCompact(EventData::with_context(
                    self.agent.clone(),
                    self.agent.model.clone(),
                    session_id,
                    transcript_path,
                    cwd,
                    post_payload,
                ));
                if let LifecycleEvent::PostCompact(post_event_data) = &post_event {
                    <PluginHookHandler<S> as EventHandle<EventData<PostCompactPayload>>>::handle(
                        &self.plugin_handler,
                        post_event_data,
                        conversation,
                    )
                    .await?;
                }
                // Drain hook_result — PostCompact extras are not
                // consumed on this path.
                let _ = std::mem::take(&mut conversation.hook_result);
            } else {
                debug!(agent_id = %self.agent.id, "Compaction not needed");
            }
        }
        Ok(())
    }
}
