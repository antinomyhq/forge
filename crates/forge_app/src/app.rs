use std::sync::Arc;

use anyhow::Result;
use chrono::Local;
use forge_domain::*;
use forge_stream::MpscStream;

use crate::orch::Orchestrator;
use crate::tool_registry::ToolRegistry;
use crate::{
    AttachmentService, ConversationService, EnvironmentService, FileDiscoveryService,
    ProviderService, Services, WorkflowService,
};

/// ForgeApp handles the core chat functionality by orchestrating various
/// services. It encapsulates the complex logic previously contained in the
/// ForgeAPI chat method.
pub struct ForgeApp<S: Services> {
    services: Arc<S>,
    tool_registry: ToolRegistry<S>,
}

impl<S: Services> ForgeApp<S> {
    /// Creates a new ForgeApp instance with the provided services.
    pub fn new(services: Arc<S>) -> Self {
        Self { tool_registry: ToolRegistry::new(services.clone()), services }
    }

    /// Executes a chat request and returns a stream of responses.
    /// This method contains the core chat logic extracted from ForgeAPI.
    pub async fn chat(
        &self,
        mut chat: ChatRequest,
    ) -> Result<MpscStream<Result<ChatResponse, anyhow::Error>>> {
        let services = self.services.clone();

        // Get the conversation for the chat request
        let conversation = services
            .conversation_service()
            .find(&chat.conversation_id)
            .await
            .unwrap_or_default()
            .expect("conversation for the request should've been created at this point.");

        // Get tool definitions and models
        let tool_definitions = self.tool_registry.list().await?;
        let models = services.provider_service().models().await?;

        // Discover files using the discovery service
        let workflow = services
            .workflow_service()
            .read(None)
            .await
            .unwrap_or_default();
        let max_depth = workflow.max_walker_depth;
        let files = services
            .file_discovery_service()
            .collect(max_depth)
            .await?
            .into_iter()
            .map(|f| f.path)
            .collect::<Vec<_>>();

        // Get environment for orchestrator creation
        let environment = services.environment_service().get_environment();

        // Always try to get attachments and overwrite them
        let attachments = services
            .attachment_service()
            .attachments(&chat.event.value.to_string())
            .await?;
        chat.event = chat.event.attachments(attachments);

        // Create the orchestrator with all necessary dependencies
        let orch = Orchestrator::new(
            services.clone(),
            environment.clone(),
            conversation,
            Local::now(),
        )
        .tool_definitions(tool_definitions)
        .models(models)
        .files(files);

        // Create and return the stream
        let stream = MpscStream::spawn(
            |tx: tokio::sync::mpsc::Sender<Result<ChatResponse, anyhow::Error>>| {
                async move {
                    let tx = Arc::new(tx);

                    // Execute dispatch and always save conversation afterwards
                    let mut orch = orch.sender(tx.clone());
                    let dispatch_result = orch.chat(chat.event).await;

                    // Always save conversation using get_conversation()
                    let conversation = orch.get_conversation().clone();
                    let save_result = services.conversation_service().upsert(conversation).await;

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
        conversation_id: &ConversationId,
    ) -> Result<CompactionResult> {
        use crate::compact::Compactor;

        // Get the conversation
        let mut conversation = self
            .services
            .conversation_service()
            .find(conversation_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Conversation not found: {}", conversation_id))?;

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
        let original_text = context.to_text();
        let original_tokens = estimate_token_count(original_text.len());

        // Find the main agent (first agent in the conversation)
        // In most cases, there should be a primary agent for compaction
        let agent = conversation
            .agents
            .first()
            .ok_or_else(|| anyhow::anyhow!("No agents found in conversation"))?
            .clone();

        // Check if the agent has compaction configured
        if agent.compact.is_none() {
            // No compaction configured, return original metrics as both original and
            // compacted
            return Ok(CompactionResult::new(
                original_tokens,
                original_tokens,
                original_messages,
                original_messages,
            ));
        }

        // Apply compaction using the Compactor
        let compactor = Compactor::new(self.services.clone());
        let compacted_context = compactor.compact_context(&agent, context).await?;

        // Calculate compacted metrics
        let compacted_messages = compacted_context.messages.len();
        let compacted_text = compacted_context.to_text();
        let compacted_tokens = estimate_token_count(compacted_text.len());

        // Update the conversation with the compacted context
        conversation.context = Some(compacted_context);

        // Save the updated conversation
        self.services
            .conversation_service()
            .upsert(conversation)
            .await?;

        // Return the compaction metrics
        Ok(CompactionResult::new(
            original_tokens,
            compacted_tokens,
            original_messages,
            compacted_messages,
        ))
    }
    pub async fn list_tools(&self) -> Result<Vec<ToolDefinition>> {
        self.tool_registry.list().await
    }
}
