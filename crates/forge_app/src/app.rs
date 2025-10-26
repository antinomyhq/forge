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
use crate::tool_resolver::ToolResolver;
use crate::{
    AgentLoaderService, AttachmentService, CommandLoaderService, ConversationService,
    EnvironmentService, FileDiscoveryService, ProviderAuthService, ProviderRegistry,
    ProviderService, Services, Walker, WorkflowService,
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

        let provider = services
            .get_active_provider()
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
        let model = services.get_active_model().await?;
        let commands = services.get_commands().await?;
        let agent = agents
            .into_iter()
            .map(|agent| {
                agent
                    .apply_workflow_config(&workflow)
                    .set_model_deeply(model.clone())
                    .subscribe_commands(&commands)
            })
            .find(|agent| agent.has_subscription(&chat.event.name))
            .ok_or(crate::Error::UnsubscribedEvent(chat.event.name.to_owned()))?;

        // Get system and mcp tool definitions and resolve them for the agent
        let all_tool_definitions = self.tool_registry.list().await?;
        let tool_resolver = ToolResolver::new(all_tool_definitions);
        let tool_definitions: Vec<ToolDefinition> =
            tool_resolver.resolve(&agent).into_iter().cloned().collect();
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
        let original_token_count = *context.token_count();
        let model = self.services.get_active_model().await?;
        let workflow = self.services.read_merged(None).await.unwrap_or_default();
        let active_agent = self.services.get_active_agent().await?;
        let Some(compact) = self
            .services
            .get_agents()
            .await?
            .into_iter()
            .find(|agent| active_agent.as_ref().is_some_and(|id| agent.id == *id))
            .and_then(|agent| {
                agent
                    .apply_workflow_config(&workflow)
                    .set_model_deeply(model.clone())
                    .compact
            })
        else {
            return Ok(CompactionResult::new(
                original_token_count,
                0,
                original_messages,
                0,
            ));
        };

        // Apply compaction using the Compactor
        let compacted_context = Compactor::new(self.services.clone(), compact)
            .compact(context, true)
            .await?;

        let compacted_messages = compacted_context.messages.len();
        let compacted_tokens = *compacted_context.token_count();

        // Update the conversation with the compacted context
        conversation.context = Some(compacted_context);

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

    /// Initializes Forge platform authentication
    pub async fn login(&self, init_auth: &InitAuth) -> Result<()> {
        self.authenticator.login(init_auth).await
    }

    /// Returns device code information for user authorization
    pub async fn init_auth(&self) -> Result<InitAuth> {
        self.authenticator.init().await
    }

    /// Logs out of Forge platform
    pub async fn logout(&self) -> Result<()> {
        self.authenticator.logout().await
    }

    /// Initiates authentication for an LLM provider
    pub async fn init_provider_auth(
        &self,
        provider_id: crate::dto::ProviderId,
        method: crate::dto::AuthMethod,
    ) -> Result<crate::dto::AuthInitiation> {
        self.services.init_provider_auth(provider_id, method).await
    }

    /// Complete provider authentication and save credentials
    /// For OAuth flows (Device/Code), this will poll until completion then save
    /// For ApiKey flows, this will use the data from AuthContext
    pub async fn complete_provider_auth(
        &self,
        provider_id: crate::dto::ProviderId,
        context: crate::dto::AuthContext,
        timeout: std::time::Duration,
        method: crate::dto::AuthMethod,
    ) -> Result<()> {
        self.services
            .complete_provider_auth(provider_id, context, timeout, method)
            .await
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
