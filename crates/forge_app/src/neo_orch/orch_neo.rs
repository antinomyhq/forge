use std::sync::Arc;

use derive_setters::Setters;
use forge_domain::*;
use tracing::debug;

use crate::agent::AgentService;
use crate::neo_orch::events::AgentAction;
use crate::neo_orch::executor::AgentExecutor;
use crate::neo_orch::programs::AgentProgramBuilder;

pub type ArcSender = Arc<tokio::sync::mpsc::Sender<anyhow::Result<ChatResponse>>>;

/// OrchNeo is the new orchestrator implementation that takes the same
/// parameters as the current Orchestrator::new method, providing a drop-in
/// replacement.
#[derive(Clone, Setters)]
#[setters(into, strip_option)]
pub struct OrchNeo<S> {
    services: Arc<S>,
    environment: Environment,
    conversation: Conversation,
    current_time: chrono::DateTime<chrono::Local>,
    tool_definitions: Vec<ToolDefinition>,
    models: Vec<Model>,
    files: Vec<String>,
    sender: Option<ArcSender>,
}

impl<S: AgentService> OrchNeo<S> {
    /// Creates a new OrchNeo instance with the same parameters as
    /// Orchestrator::new
    pub fn new(
        services: Arc<S>,
        environment: Environment,
        conversation: Conversation,
        current_time: chrono::DateTime<chrono::Local>,
    ) -> Self {
        Self {
            services,
            environment,
            conversation,
            current_time,
            tool_definitions: Default::default(),
            models: Default::default(),
            files: Default::default(),
            sender: None,
        }
    }

    /// Get a reference to the internal conversation
    pub fn get_conversation(&self) -> &Conversation {
        &self.conversation
    }

    /// Execute a chat event using the neo_orch architecture
    pub async fn chat(&mut self, event: Event) -> anyhow::Result<()> {
        let target_agents = {
            debug!(
                conversation_id = %self.conversation.id.clone(),
                event_name = %event.name,
                event_value = ?event.value,
                "Dispatching event"
            );
            self.conversation.dispatch_event(event.clone())
        };

        // Execute all agent initialization with the event
        for agent_id in &target_agents {
            self.init_agent(agent_id, &event).await?;
        }

        Ok(())
    }

    /// Initialize and execute a specific agent with the given event using
    /// AgentExecutor
    async fn init_agent(&mut self, agent_id: &AgentId, event: &Event) -> anyhow::Result<()> {
        debug!(
            conversation_id = %self.conversation.id,
            agent = %agent_id,
            event = ?event,
            "Initializing agent"
        );

        let agent = self.conversation.get_agent(agent_id)?.clone();
        let model_id = agent
            .model
            .clone()
            .ok_or(Error::MissingModel(agent.id.clone()))?;

        // Find the model for this agent
        let model = self
            .models
            .iter()
            .find(|m| m.id == model_id)
            .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?
            .clone();

        // Create the agent program
        let program = AgentProgramBuilder::default()
            .tool_definitions(self.tool_definitions.clone())
            .agent(agent.clone())
            .model(model)
            .environment(self.environment.clone())
            .files(self.files.clone())
            .current_time(self.current_time)
            .build()?;

        // Initialize context
        let mut context = self.conversation.context.clone().unwrap_or_default();
        context = context.conversation_id(self.conversation.id);

        // Create the agent executor with all necessary configuration
        let executor = AgentExecutor::with_context(self.services.clone(), program, context)
            .sender(self.sender.clone())
            .models(self.models.clone())
            .conversation_id(self.conversation.id)
            .max_requests_per_turn(self.conversation.max_requests_per_turn);

        // Let the executor handle the complete execution loop
        let initial_action = AgentAction::ChatEvent(event.clone());
        executor.run(initial_action, &agent).await?;

        // Update conversation context from executor's final state
        self.conversation.context = Some(executor.get_final_context().await);

        Ok(())
    }
}
