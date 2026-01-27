//! Specialized executor for the codebase_search agent.
//!
//! This module provides execution logic for the codebase_search agent with
//! iteration limiting and output capture through hooks.

use std::sync::Arc;

use forge_domain::{
    AgentId, ChatRequest, ChatResponse, ChatResponseContent, Conversation, Event, TitleFormat,
    ToolCallContext, ToolOutput,
};
use futures::StreamExt;
use tokio::sync::Mutex;

use crate::error::Error;
use crate::{hooks, ConversationService, EnvironmentService, Services};

#[derive(Clone)]
pub struct CodebaseSearchExecutor<S> {
    services: Arc<S>,
}

impl<S: Services> CodebaseSearchExecutor<S> {
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }

    /// Executes the codebase_search agent with iteration limiting and output
    /// capture.
    pub async fn execute(
        &self,
        agent_id: AgentId,
        task: String,
        ctx: &ToolCallContext,
    ) -> anyhow::Result<ToolOutput> {
        ctx.send_tool_input(
            TitleFormat::debug(format!("Codebase Search",)).sub_title(task.as_str()),
        )
        .await?;

        // Create a new conversation for agent execution
        let conversation = Conversation::generate().title(task.clone());
        self.services
            .conversation_service()
            .upsert_conversation(conversation.clone())
            .await?;

        // Create hooks for iteration limiting and output capture
        let env = self.services.get_environment();
        let captured_output = Arc::new(Mutex::new(None));
        let tool_name = forge_domain::ToolName::new("report_search");

        let hook = hooks::tool_call_reminder(
            agent_id.clone(),
            tool_name.clone(),
            env.codebase_search_max_iterations,
        )
        .zip(hooks::tool_output_capture(
            agent_id.clone(),
            tool_name,
            captured_output.clone(),
        ));

        // Execute the request through ForgeApp with both hooks merged
        let app = crate::ForgeApp::<S>::new(self.services.clone()).with_hook(Arc::new(hook));
        let request = ChatRequest::new(Event::new(task.clone()), conversation.id);
        let mut response_stream = app.chat(agent_id.clone(), request).await?;

        // Collect responses from the agent
        while let Some(message) = response_stream.next().await {
            let message = message?;
            match message {
                ChatResponse::TaskMessage { ref content } => match content {
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
            }
        }

        // Prefer the captured tool result output, fall back to text output
        if let Some(result) = captured_output.lock().await.take() {
            Ok(ToolOutput::ai(
                conversation.id,
                result.output.as_str().unwrap_or(""),
            ))
        } else {
            Err(Error::EmptyToolResponse.into())
        }
    }
}
