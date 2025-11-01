use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use chrono::Local;
use forge_domain::{InitAuth, *};
use forge_stream::MpscStream;

use crate::apply_tunable_parameters::ApplyTunableParameters;
use crate::authenticator::Authenticator;
use crate::dto::ToolsOverview;
use crate::init_conversation_metrics::InitConversationMetrics;
use crate::orch::Orchestrator;
use crate::services::{AppConfigService, CustomInstructionsService, TemplateService};
use crate::set_conversation_id::SetConversationId;
use crate::system_prompt::SystemPrompt;
use crate::tool_registry::ToolRegistry;
use crate::tool_resolver::ToolResolver;
use crate::user_prompt::UserPromptGenerator;
use crate::{
    AgentRegistry, ConversationService, EnvironmentService, FileDiscoveryService, ProviderService,
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
        agent_id: AgentId,
        chat: ChatRequest,
    ) -> Result<MpscStream<Result<ChatResponse, anyhow::Error>>> {
        let services = self.services.clone();

        // Get the conversation for the chat request
        let conversation = services
            .find_conversation(&chat.conversation_id)
            .await
            .unwrap_or_default()
            .expect("conversation for the request should've been created at this point.");

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

        let custom_instructions = services.get_custom_instructions().await;

        // Prepare agents with user configuration
        let active_model = self.get_model(Some(agent_id.clone())).await?;
        let agent = services
            .get_agents()
            .await?
            .into_iter()
            .map(|agent| {
                agent
                    .apply_workflow_config(&workflow)
                    .set_model_deeply(active_model.clone())
            })
            .find(|agent| agent.id == agent_id)
            .ok_or(crate::Error::AgentNotFound(agent_id))?;

        let agent_provider = self.get_provider(Some(agent.id.clone())).await?;
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
                .add_system_message(conversation)
                .await?;

        // Insert user prompt
        let conversation = UserPromptGenerator::new(
            self.services.clone(),
            agent.clone(),
            chat.event.clone(),
            current_time,
        )
        .add_user_prompt(conversation)
        .await?;

        let conversation = InitConversationMetrics::new(current_time).apply(conversation);
        let conversation = ApplyTunableParameters::new(agent.clone(), tool_definitions.clone())
            .apply(conversation);
        let conversation = SetConversationId.apply(conversation);

        // Create the orchestrator with all necessary dependencies
        let orch = Orchestrator::new(
            services.clone(),
            environment.clone(),
            conversation,
            agent,
            chat.event,
        )
        .error_tracker(ToolErrorTracker::new(max_tool_failure_per_turn))
        .tool_definitions(tool_definitions)
        .models(models);

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
        let active_agent_id = self.services.get_active_agent_id().await?;
        let model = self.get_model(active_agent_id.clone()).await?;
        let workflow = self.services.read_merged(None).await.unwrap_or_default();
        let Some(compact) = self
            .services
            .get_agents()
            .await?
            .into_iter()
            .find(|agent| active_agent_id.as_ref().is_some_and(|id| agent.id == *id))
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

    pub async fn get_provider(&self, agent: Option<AgentId>) -> anyhow::Result<Provider> {
        if let Some(agent) = agent
            && let Some(agent) = self.services.get_agent(&agent).await?
            && let Some(provider_id) = agent.provider
        {
            return self.services.get_provider(provider_id).await;
        }

        // Fall back to original logic if there is no agent
        // set yet.
        self.services.get_default_provider().await
    }

    /// Gets the model for the specified agent, or the default model if no agent
    /// is provided
    pub async fn get_model(&self, agent_id: Option<AgentId>) -> anyhow::Result<ModelId> {
        let provider_id = self.get_provider(agent_id).await?.id;
        self.services.get_default_model(&provider_id).await
    }

    pub async fn set_default_model(
        &self,
        agent_id: Option<AgentId>,
        model: ModelId,
    ) -> anyhow::Result<()> {
        let provider_id = self.get_provider(agent_id).await?.id;
        self.services.set_default_model(model, provider_id).await
    }
}
