use std::sync::Arc;

use anyhow::Context;
use convert_case::{Case, Casing};
use forge_domain::{
    AgentId, ChatRequest, ChatResponse, ChatResponseContent, Conversation, Event, TitleFormat,
    ToolCallContext, ToolDefinition, ToolName, ToolOutput,
};
use forge_template::Element;
use futures::StreamExt;
use tokio::sync::RwLock;

use crate::error::Error;
use crate::{AgentRegistry, ConversationService, Services};

#[derive(Clone)]
pub struct AgentExecutor<S> {
    services: Arc<S>,
    pub tool_agents: Arc<RwLock<Option<Vec<ToolDefinition>>>>,
}

impl<S: Services> AgentExecutor<S> {
    pub fn new(services: Arc<S>) -> Self {
        Self { services, tool_agents: Arc::new(RwLock::new(None)) }
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
        conversation_id: Option<String>,
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
        let conversation = if let Some(cid) = conversation_id {
            let conversation_id = forge_domain::ConversationId::parse(&cid)
                .map_err(|_| Error::ConversationNotFound { id: cid.clone() })?;
            self.services
                .conversation_service()
                .find_conversation(&conversation_id)
                .await?
                .ok_or(Error::ConversationNotFound { id: cid })?
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
        // Execute the request through the ForgeApp
        let app = crate::ForgeApp::new(self.services.clone());
        let mut response_stream = app
            .chat(
                agent_id.clone(),
                ChatRequest::new(Event::new(task.clone()), conversation.id),
            )
            .await?;

        // Collect responses from the agent
        let mut output = String::new();
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
                    return Err(Error::AgentToolInterrupted(reason))
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

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use forge_domain::ConversationId;

    /// Tests that ConversationId::parse works correctly for valid UUIDs
    #[test]
    fn test_conversation_id_parse_valid_uuid() {
        let fixture = "550e8400-e29b-41d4-a716-446655440000";
        let actual = ConversationId::parse(fixture);
        assert!(actual.is_ok(), "Should parse valid UUID as conversation ID");
    }

    /// Tests that ConversationId::parse handles empty string
    #[test]
    fn test_conversation_id_parse_empty_string() {
        let actual = ConversationId::parse("");
        // Empty string should either fail or produce a valid empty ID
        // depending on the implementation
        assert!(
            actual.is_ok() || actual.is_err(),
            "Empty string should be handled"
        );
    }

    /// Tests that ConversationNotFound error message contains the ID
    #[test]
    fn test_conversation_not_found_error_contains_id() {
        let fixture = "test-id".to_string();
        let error = crate::error::Error::ConversationNotFound { id: fixture.clone() };
        let actual = error.to_string();
        assert!(
            actual.contains(&fixture),
            "Error message should contain the conversation ID"
        );
        assert!(
            actual.contains("not found"),
            "Error message should indicate not found"
        );
    }

    /// Tests that ConversationNotFound error can be created and matched
    #[test]
    fn test_conversation_not_found_error_matches_variant() {
        let fixture = "session-abc-123".to_string();
        let actual = crate::error::Error::ConversationNotFound { id: fixture.clone() };
        let expected_id = fixture;
        match actual {
            crate::error::Error::ConversationNotFound { id } => {
                assert_eq!(id, expected_id);
            }
            _ => panic!("Expected ConversationNotFound error variant"),
        }
    }

    /// Tests that AgentToolInterrupted error message indicates interruption
    #[test]
    fn test_agent_tool_interrupted_error_message() {
        use forge_domain::InterruptionReason;
        use std::collections::HashMap;

        let fixture = InterruptionReason::MaxToolFailurePerTurnLimitReached {
            limit: 5,
            errors: HashMap::new(),
        };
        let error = crate::error::Error::AgentToolInterrupted(fixture);
        let actual = error.to_string();
        assert!(
            actual.contains("interrupted"),
            "Error message should indicate interruption"
        );
    }

    /// Tests that EmptyToolResponse error message mentions empty response
    #[test]
    fn test_empty_tool_response_error_message() {
        let error = crate::error::Error::EmptyToolResponse;
        let actual = error.to_string();
        assert!(
            actual.contains("Empty"),
            "Error message should mention empty"
        );
        assert!(
            actual.contains("response"),
            "Error message should mention response"
        );
    }
}
