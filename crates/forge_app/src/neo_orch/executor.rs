use std::collections::HashMap;
use std::sync::Arc;

use async_recursion::async_recursion;
use forge_domain::*;
use tokio::sync::Mutex;
use tracing::{debug, warn};

use crate::agent::AgentService;
use crate::neo_orch::events::{AgentAction, AgentCommand};
use crate::neo_orch::program::Program;
use crate::neo_orch::state::AgentState;

pub struct AgentExecutor<S, P> {
    services: Arc<S>,
    program: P,
    state: Mutex<AgentState>,
    sender: Option<Arc<tokio::sync::mpsc::Sender<anyhow::Result<ChatResponse>>>>,
    models: Vec<Model>,
    conversation_id: ConversationId,
    max_requests_per_turn: Option<usize>,
}

impl<
    S: AgentService + Send + Sync,
    P: Program<
            Action = AgentAction,
            State = AgentState,
            Error = anyhow::Error,
            Success = AgentCommand,
        > + Send
        + Sync,
> AgentExecutor<S, P>
{
    pub fn new(services: Arc<S>, program: P) -> AgentExecutor<S, P> {
        Self {
            services,
            program,
            state: Mutex::new(AgentState::default()),
            sender: None,
            models: Vec::new(),
            conversation_id: ConversationId::generate(),
            max_requests_per_turn: None,
        }
    }

    pub fn with_context(services: Arc<S>, program: P, context: Context) -> AgentExecutor<S, P> {
        Self {
            services,
            program,
            state: Mutex::new(AgentState::new(context)),
            sender: None,
            models: Vec::new(),
            conversation_id: ConversationId::generate(),
            max_requests_per_turn: None,
        }
    }

    pub fn sender(
        mut self,
        sender: Option<Arc<tokio::sync::mpsc::Sender<anyhow::Result<ChatResponse>>>>,
    ) -> Self {
        self.sender = sender;
        self
    }

    pub fn models(mut self, models: Vec<Model>) -> Self {
        self.models = models;
        self
    }

    pub fn conversation_id(mut self, conversation_id: ConversationId) -> Self {
        self.conversation_id = conversation_id;
        self
    }

    pub fn max_requests_per_turn(mut self, max_requests_per_turn: Option<usize>) -> Self {
        self.max_requests_per_turn = max_requests_per_turn;
        self
    }

    /// Execute an action using the program and return the resulting command
    pub async fn execute(&self, action: AgentAction) -> anyhow::Result<AgentCommand> {
        let mut state = self.state.lock().await;
        self.program.update(&action, &mut state)
    }

    /// Run the complete execution loop starting with an initial action
    pub async fn run(&self, initial_action: AgentAction, agent: &Agent) -> anyhow::Result<()> {
        let mut tool_failure_attempts = HashMap::new();
        let mut request_count = 0;

        // Process the initial action
        let mut current_command = self.execute(initial_action).await?;

        loop {
            // Process the current command and get the next action
            let next_action = self
                .process_command(current_command, agent, &mut tool_failure_attempts)
                .await?;

            // Check if we're done
            if let Some(action) = next_action {
                // Execute the next action
                current_command = self.execute(action).await?;

                // Check request limits
                request_count += 1;
                if let Some(max_requests) = self.max_requests_per_turn
                    && request_count >= max_requests
                {
                    warn!(
                        agent_id = %agent.id,
                        request_count,
                        max_requests_per_turn = max_requests,
                        "Maximum requests per turn exceeded"
                    );
                    break;
                }
            } else {
                // No more actions to process, we're done
                break;
            }
        }

        Ok(())
    }

    /// Get the final context from the executor's state
    pub async fn get_final_context(&self) -> Context {
        let state = self.state.lock().await;
        state.context.clone()
    }

    /// Process a command and execute its side effects, returning the next
    /// action if any
    #[async_recursion]
    async fn process_command(
        &self,
        command: AgentCommand,
        agent: &Agent,
        tool_failure_attempts: &mut HashMap<String, usize>,
    ) -> anyhow::Result<Option<AgentAction>> {
        match command {
            AgentCommand::Chat { model, context } => {
                // Execute chat completion
                let completion = self.execute_chat_completion(&model, context, agent).await?;

                // Return the completion as the next action
                Ok(Some(AgentAction::ChatCompletionMessage(Ok(completion))))
            }
            AgentCommand::ToolCall { call } => {
                // Execute tool call
                let tool_result = self.execute_single_tool_call(agent, &call).await?;

                // Return the tool result as the next action
                Ok(Some(AgentAction::ToolResult(tool_result)))
            }
            AgentCommand::ChatResponse(response) => {
                // Send chat response through the sender if available
                self.send(response).await?;
                Ok(None) // No next action
            }
            AgentCommand::Render { id, template, object } => {
                // Render template
                let rendered_content = self.services.render(&template, &object).await?;

                // Return the render result as the next action
                Ok(Some(AgentAction::RenderResult {
                    id,
                    content: rendered_content,
                }))
            }
            AgentCommand::Combine(left, right) => {
                // Process both commands
                let left_action = self
                    .process_command(*left, agent, tool_failure_attempts)
                    .await?;
                let right_action = self
                    .process_command(*right, agent, tool_failure_attempts)
                    .await?;

                // Return the first non-None action, or None if both are None
                Ok(left_action.or(right_action))
            }
            AgentCommand::Empty => {
                // Nothing to do
                Ok(None)
            }
        }
    }

    /// Execute a chat completion
    async fn execute_chat_completion(
        &self,
        model_id: &ModelId,
        context: Context,
        agent: &Agent,
    ) -> anyhow::Result<ChatCompletionMessageFull> {
        let tool_supported = self.is_tool_supported(agent)?;
        let reasoning_supported = self.is_reasoning_supported(agent)?;

        let mut transformers = TransformToolCalls::new()
            .when(|_| !tool_supported)
            .pipe(ImageHandling::new())
            .pipe(DropReasoningDetails.when(|_| !reasoning_supported))
            .pipe(ReasoningNormalizer.when(|_| reasoning_supported));

        let response = self
            .services
            .chat_agent(model_id, transformers.transform(context))
            .await?;
        response.into_full(!tool_supported).await
    }

    /// Execute a single tool call
    async fn execute_single_tool_call(
        &self,
        agent: &Agent,
        tool_call: &ToolCallFull,
    ) -> anyhow::Result<ToolResult> {
        // Send the start notification
        self.send(ChatResponse::ToolCallStart(tool_call.clone()))
            .await?;

        // Create tool context
        let mut tool_context =
            ToolCallContext::new(TaskList::default()).sender(self.sender.clone());

        // Execute the tool
        let tool_result = self
            .services
            .call(agent, &mut tool_context, tool_call.clone())
            .await;

        if tool_result.is_error() {
            warn!(
                agent_id = %agent.id,
                name = %tool_call.name,
                arguments = %tool_call.arguments,
                output = ?tool_result.output,
                "Tool call failed",
            );
        }

        // Send the end notification
        self.send(ChatResponse::ToolCallEnd(tool_result.clone()))
            .await?;

        Ok(tool_result)
    }

    /// Send a message through the sender if available
    async fn send(&self, message: ChatResponse) -> anyhow::Result<()> {
        if let Some(sender) = &self.sender {
            sender.send(Ok(message)).await?
        }
        Ok(())
    }

    /// Returns if agent supports tool or not.
    fn is_tool_supported(&self, agent: &Agent) -> anyhow::Result<bool> {
        let model_id = agent
            .model
            .as_ref()
            .ok_or(Error::MissingModel(agent.id.clone()))?;

        // Check if at agent level tool support is defined
        let tool_supported = match agent.tool_supported {
            Some(tool_supported) => tool_supported,
            None => {
                // If not defined at agent level, check model level
                let model = self.models.iter().find(|model| &model.id == model_id);
                model
                    .and_then(|model| model.tools_supported)
                    .unwrap_or_default()
            }
        };

        debug!(
            agent_id = %agent.id,
            model_id = %model_id,
            tool_supported,
            "Tool support check"
        );
        Ok(tool_supported)
    }

    fn is_reasoning_supported(&self, agent: &Agent) -> anyhow::Result<bool> {
        let model_id = agent
            .model
            .as_ref()
            .ok_or(Error::MissingModel(agent.id.clone()))?;

        let model = self.models.iter().find(|model| &model.id == model_id);
        let reasoning_supported = model
            .and_then(|model| model.supports_reasoning)
            .unwrap_or_default();

        debug!(
            agent_id = %agent.id,
            model_id = %model_id,
            reasoning_supported,
            "Reasoning support check"
        );
        Ok(reasoning_supported)
    }
}
