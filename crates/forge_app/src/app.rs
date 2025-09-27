use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Local;
use forge_domain::*;
use forge_stream::MpscStream;

use crate::authenticator::Authenticator;
use crate::dto::{InitAuth, ToolsOverview};
use crate::orch::Orchestrator;
use crate::services::{CustomInstructionsService, TemplateService};
use crate::tool_registry::ToolRegistry;
use crate::{
    AgentLoaderService, AppConfigService, AttachmentService, ConversationService,
    EnvironmentService, FileDiscoveryService, McpService, ProviderRegistry, ProviderService,
    Services, Walker, WorkflowService,
};

/// ForgeApp handles the core chat functionality by orchestrating various
/// services. It encapsulates the complex logic previously contained in the
/// ForgeAPI chat method.
pub struct ForgeApp<S> {
    services: Arc<S>,
    tool_registry: ToolRegistry<S>,
    authenticator: Authenticator<S>,
}

impl<S: Services> ForgeApp<S> {
    /// Creates a new ForgeApp instance with the provided services.
    pub fn new(services: Arc<S>) -> Self {
        Self {
            tool_registry: ToolRegistry::new(services.clone()),
            authenticator: Authenticator::new(services.clone()),
            services,
        }
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
            .find_conversation(&chat.conversation_id)
            .await
            .unwrap_or_default()
            .expect("conversation for the request should've been created at this point.");

        let config = services.get_app_config().await.unwrap_or_default();
        let provider = services
            .get_provider(config)
            .await
            .context("Failed to get provider")?;
        let models = services.models(provider).await?;

        // Discover files using the discovery service
        let workflow = self.services.read_merged(None).await.unwrap_or_default();
        let max_depth = workflow.max_walker_depth;
        let environment = services.get_environment();

        let mut walker = Walker::conservative().cwd(environment.cwd.clone());

        if let Some(depth) = max_depth {
            walker = walker.max_depth(depth);
        };

        let files = services
            .collect_files(walker)
            .await?
            .into_iter()
            .filter(|f| !f.is_dir)
            .map(|f| f.path)
            .collect::<Vec<_>>();

        // Register templates using workflow path or environment fallback
        let template_path = workflow
            .templates
            .as_ref()
            .map_or(environment.templates(), |templates| {
                PathBuf::from(templates)
            });

        services.register_template(template_path).await?;

        // Always try to get attachments and overwrite them
        if let Some(value) = chat.event.value.as_ref() {
            let attachments = services.attachments(&value.to_string()).await?;
            chat.event = chat.event.attachments(attachments);
        }

        let custom_instructions = services.get_custom_instructions().await;

        // Prepare agents with user configuration and subscriptions
        let agents = services.get_agents().await?;

        let mcp_tools = self.services.mcp_service().list().await?;
        let agent = agents
            .into_iter()
            .map(|agent| {
                agent
                    .apply_workflow_config(&workflow)
                    .extend_mcp_tools(&mcp_tools)
            })
            .find(|agent| agent.has_subscription(&chat.event.name))
            .ok_or(crate::Error::UnsubscribedEvent(chat.event.name.to_owned()))?;

        // Get system and mcp tool definitions
        let all_tool_definitions = self.tool_registry.list().await?;
        let tool_definitions = agent.resolve_tool_definitions(&all_tool_definitions);
        let max_tool_failure_per_turn = agent.max_tool_failure_per_turn.unwrap_or(3);

        // Create the orchestrator with all necessary dependencies
        let orch = Orchestrator::new(
            services.clone(),
            environment.clone(),
            conversation,
            Local::now(),
            agent,
            chat.event,
        )
        .error_tracker(ToolErrorTracker::new(max_tool_failure_per_turn))
        .custom_instructions(custom_instructions)
        .tool_definitions(tool_definitions)
        .models(models)
        .files(files);

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
        let original_tokens_approx = context.token_count_approx();

        // Find the main agent (first agent in the conversation)
        // In most cases, there should be a primary agent for compaction
        let agent = self
            .services
            .get_agents()
            .await?
            .first()
            .ok_or_else(|| anyhow::anyhow!("No agents found in conversation"))?
            .clone();

        // Apply compaction using the Compactor
        let compactor = Compactor::new(self.services.clone());

        let compacted_context = compactor.compact(&agent, context, true).await?;

        // Calculate compacted metrics
        let compacted_messages = compacted_context.messages.len();
        let compacted_tokens_approx = compacted_context.token_count_approx();

        // Update the conversation with the compacted context
        conversation.context = Some(compacted_context);

        // Save the updated conversation
        self.services.upsert_conversation(conversation).await?;

        // Return the compaction metrics
        Ok(CompactionResult::new(
            original_tokens_approx,
            compacted_tokens_approx,
            original_messages,
            compacted_messages,
        ))
    }

    pub async fn list_tools(&self) -> Result<ToolsOverview> {
        self.tool_registry.tools_overview().await
    }
    pub async fn login(&self, init_auth: &InitAuth) -> Result<()> {
        self.authenticator.login(init_auth).await
    }
    pub async fn init_auth(&self) -> Result<InitAuth> {
        self.authenticator.init().await
    }
    pub async fn logout(&self) -> Result<()> {
        self.authenticator.logout().await
    }
    pub async fn read_workflow(&self, path: Option<&Path>) -> Result<Workflow> {
        self.services.read_workflow(path).await
    }

    pub async fn read_workflow_merged(&self, path: Option<&Path>) -> Result<Workflow> {
        self.services.read_merged(path).await
    }
    pub async fn write_workflow(&self, path: Option<&Path>, workflow: &Workflow) -> Result<()> {
        self.services.write_workflow(path, workflow).await
    }
}
