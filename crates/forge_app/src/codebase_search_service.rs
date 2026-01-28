use std::sync::Arc;

use forge_domain::{
    AgentId, ChatRequest, ChatResponse, ChatResponseContent, Conversation, Event, Exit,
    ToolCallContext, ToolOutput,
};
use futures::StreamExt;

use crate::error::Error;
use crate::{ConversationService, EnvironmentService, Services, hooks};

#[derive(Clone)]
pub struct CodebaseSearchService<S> {
    services: Arc<S>,
}

impl<S: Services> CodebaseSearchService<S> {
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }

    /// Executes the codebase_search agent with iteration limiting and output
    /// capture.
    pub async fn execute(&self, task: String, ctx: &ToolCallContext) -> anyhow::Result<ToolOutput> {
        // Create a new conversation for agent execution
        let conversation = Conversation::generate().title(task.clone());
        self.services
            .conversation_service()
            .upsert_conversation(conversation.clone())
            .await?;

        // Create hooks for iteration limiting and output capture
        let env = self.services.get_environment();
        let tool_name = forge_domain::ToolName::new("report_search");
        let agent_id = AgentId::new("codebase_search");

        let hook = hooks::tool_output_capture(agent_id.clone(), tool_name.clone()).zip(
            hooks::tool_call_reminder(
                agent_id.clone(),
                tool_name,
                env.codebase_search_max_iterations,
            ),
        );

        // Execute the request through ForgeApp with both hooks merged
        let app = crate::ForgeApp::<S>::new(self.services.clone()).with_hook(Arc::new(hook));
        let request = ChatRequest::new(Event::new(task.clone()), conversation.id);
        let mut response_stream = app.chat(agent_id.clone(), request).await?;

        // Collect responses from the agent and capture Exit
        let mut exit: Option<Exit> = None;
        while let Some(message) = response_stream.next().await {
            let message = message?;
            match &message {
                ChatResponse::TaskMessage { content } => match content {
                    ChatResponseContent::ToolInput(_) => ctx.send(message).await?,
                    ChatResponseContent::ToolOutput(_) => {}
                    ChatResponseContent::Markdown { .. } => {}
                },
                ChatResponse::TaskReasoning { .. } => {}
                ChatResponse::TaskComplete => {}
                ChatResponse::ToolCallStart(_) => ctx.send(message).await?,
                ChatResponse::ToolCallEnd(_) => ctx.send(message).await?,
                ChatResponse::RetryAttempt { .. } => ctx.send(message).await?,
                ChatResponse::Interrupt { .. } => ctx.send(message).await?,
                ChatResponse::Exit(e) => {
                    exit = Some(e.clone());
                }
            }
        }

        // Extract tool result from Exit
        let result = exit
            .and_then(|e| e.as_tool_result().cloned())
            .ok_or(Error::EmptyToolResponse)?;

        Ok(ToolOutput::ai(
            conversation.id,
            result.output.as_str().unwrap_or(""),
        ))
    }
}
