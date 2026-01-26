use std::sync::Arc;

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
    /// specified agent.
    pub async fn execute(
        &self,
        agent_id: AgentId,
        task: String,
        ctx: &ToolCallContext,
    ) -> anyhow::Result<ToolOutput> {
        ctx.send_tool_input(
            TitleFormat::debug(format!(
                "{} [Agent]",
                agent_id.as_str().to_case(Case::UpperSnake)
            ))
            .sub_title(task.as_str()),
        )
        .await?;

        // Create a new conversation for agent execution
        let conversation = Conversation::generate().title(task.clone());
        self.services
            .conversation_service()
            .upsert_conversation(conversation.clone())
            .await?;
        // Execute the request through the ForgeApp
        let app = crate::ForgeApp::new(self.services.clone());
        let mut response_stream = app
            .chat(
                agent_id.clone(),
                ChatRequest::new(Event::new(task.clone()), conversation.id),
            )
            .await?;

        // Collect responses from the agent
        let mut output = AccumulatedContent::default();
        while let Some(message) = response_stream.next().await {
            let message = message?;
            if matches!(
                &message,
                ChatResponse::ToolCallStart(_) | ChatResponse::ToolCallEnd(_)
            ) {
                output = output.reset();
            }
            match message {
                ChatResponse::TaskMessage { ref content, partial } => match content {
                    ChatResponseContent::ToolInput(_) => ctx.send(message).await?,
                    ChatResponseContent::ToolOutput(text) => {
                        if partial {
                            output = output.append_tool_output(text);
                        } else {
                            output = AccumulatedContent::tool_output(text);
                        }
                    }
                    ChatResponseContent::Markdown(text) => {
                        if partial {
                            output = output.append_markdown(text);
                        } else {
                            output = AccumulatedContent::markdown(text);
                        }
                    }
                },
                ChatResponse::TaskReasoning { .. } => {}
                ChatResponse::TaskComplete => {}
                ChatResponse::ToolCallStart(_) => ctx.send(message).await?,
                ChatResponse::ToolCallEnd(_) => ctx.send(message).await?,
                ChatResponse::RetryAttempt { .. } => ctx.send(message).await?,
                ChatResponse::Interrupt { .. } => ctx.send(message).await?,
            }
        }
        if let Some(text) = output.into_text() {
            // Create tool output
            Ok(ToolOutput::ai(
                conversation.id,
                Element::new("task_completed")
                    .attr("task", &task)
                    .append(Element::new("output").text(text)),
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

#[derive(Debug, PartialEq)]
enum AccumulatedContent {
    ToolOutput(String),
    Markdown(String),
}

impl Default for AccumulatedContent {
    fn default() -> Self {
        Self::ToolOutput(String::new())
    }
}

impl AccumulatedContent {
    fn markdown(text: &str) -> Self {
        Self::Markdown(text.into())
    }

    fn tool_output(text: &str) -> Self {
        Self::ToolOutput(text.into())
    }

    /// Appends tool output text to the output.
    /// If currently in Markdown mode, switches to ToolOutput and replaces
    /// content.
    fn append_tool_output(self, text: &str) -> Self {
        match self {
            Self::ToolOutput(mut content) => {
                content.push_str(text);
                Self::ToolOutput(content)
            }
            Self::Markdown(_) => Self::ToolOutput(text.to_string()),
        }
    }

    /// Appends markdown to the output.
    /// If currently in ToolOutput mode, switches to Markdown and replaces
    /// content.
    fn append_markdown(self, text: &str) -> Self {
        match self {
            Self::Markdown(mut content) => {
                content.push_str(text);
                Self::Markdown(content)
            }
            Self::ToolOutput(_) => Self::Markdown(text.to_string()),
        }
    }

    fn reset(self) -> Self {
        Self::default()
    }

    /// Returns the accumulated text, or None if empty.
    fn into_text(self) -> Option<String> {
        match self {
            Self::ToolOutput(text) | Self::Markdown(text) if !text.is_empty() => Some(text),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_default_and_empty_content() {
        // Default creates empty ToolOutput
        let default = AccumulatedContent::default();
        assert_eq!(default, AccumulatedContent::ToolOutput(String::new()));

        // Empty content of both types returns None
        assert_eq!(AccumulatedContent::default().into_text(), None);
        assert_eq!(
            AccumulatedContent::ToolOutput(String::new()).into_text(),
            None
        );
        assert_eq!(
            AccumulatedContent::Markdown(String::new()).into_text(),
            None
        );

        // Reset from empty content returns empty content
        let reset_empty = AccumulatedContent::default().reset();
        assert_eq!(reset_empty, AccumulatedContent::ToolOutput(String::new()));
        assert_eq!(reset_empty.into_text(), None);
    }

    #[test]
    fn test_plain_text_accumulation() {
        // Single append
        let actual = AccumulatedContent::default().append_tool_output("Hello");
        assert_eq!(actual, AccumulatedContent::ToolOutput("Hello".to_string()));

        // Multiple appends accumulate
        let actual = AccumulatedContent::default()
            .append_tool_output("Hello")
            .append_tool_output(" ")
            .append_tool_output("World");
        assert_eq!(
            actual,
            AccumulatedContent::ToolOutput("Hello World".to_string())
        );

        // Non-empty content is extractable
        assert_eq!(actual.into_text(), Some("Hello World".to_string()));

        // Reset from ToolOutput with content returns empty ToolOutput
        let content = AccumulatedContent::default()
            .append_tool_output("Some text")
            .append_tool_output(" more text");
        let reset_content = content.reset();
        assert_eq!(reset_content, AccumulatedContent::ToolOutput(String::new()));
        assert_eq!(reset_content.into_text(), None);
    }

    #[test]
    fn test_markdown_accumulation() {
        // Single append
        let actual = AccumulatedContent::default().append_markdown("**Bold**");
        assert_eq!(actual, AccumulatedContent::Markdown("**Bold**".to_string()));

        // Multiple appends accumulate
        let actual = AccumulatedContent::default()
            .append_markdown("**Bold**")
            .append_markdown(" and ")
            .append_markdown("*italic*");
        assert_eq!(
            actual,
            AccumulatedContent::Markdown("**Bold** and *italic*".to_string())
        );

        // Non-empty content is extractable
        assert_eq!(
            actual.into_text(),
            Some("**Bold** and *italic*".to_string())
        );

        // Reset from Markdown with content returns empty ToolOutput
        let content = AccumulatedContent::default()
            .append_markdown("**Bold**")
            .append_markdown(" and *italic*");
        let reset_content = content.reset();
        assert_eq!(reset_content, AccumulatedContent::ToolOutput(String::new()));
        assert_eq!(reset_content.into_text(), None);
    }

    #[test]
    fn test_mode_switching() {
        // Switching from ToolOutput to Markdown replaces content
        let actual = AccumulatedContent::default()
            .append_tool_output("Old text")
            .append_markdown("**New content**");
        assert_eq!(
            actual,
            AccumulatedContent::Markdown("**New content**".to_string())
        );

        // Switching from Markdown to ToolOutput replaces content
        let actual = AccumulatedContent::default()
            .append_markdown("**Old**")
            .append_tool_output("New content");
        assert_eq!(
            actual,
            AccumulatedContent::ToolOutput("New content".to_string())
        );

        // Multiple switches only keep last content
        let actual = AccumulatedContent::default()
            .append_tool_output("First")
            .append_markdown("**Second**")
            .append_tool_output("Third")
            .append_markdown("**Fourth**");
        assert_eq!(
            actual,
            AccumulatedContent::Markdown("**Fourth**".to_string())
        );

        // Reset after mode switches returns empty ToolOutput
        let content = AccumulatedContent::default()
            .append_tool_output("First")
            .append_markdown("**Second**")
            .append_tool_output("Third");
        let reset_content = content.reset();
        assert_eq!(reset_content, AccumulatedContent::ToolOutput(String::new()));
        assert_eq!(reset_content.into_text(), None);
    }

    #[test]
    fn test_comprehensive_workflow() {
        // Realistic workflow: accumulate, switch, extract
        let content = AccumulatedContent::default()
            .append_tool_output("Start with plain text")
            .append_tool_output(" and continue")
            .append_markdown("\n\n**Switch to markdown**")
            .append_markdown(" with more content")
            .append_tool_output("\nBack to plain text");

        // Only the last mode's content is kept
        assert_eq!(
            content,
            AccumulatedContent::ToolOutput("\nBack to plain text".to_string())
        );

        // Extract the final content
        assert_eq!(
            content.into_text(),
            Some("\nBack to plain text".to_string())
        );

        // Multiple resets work correctly in workflow
        let content = AccumulatedContent::default()
            .append_tool_output("Test")
            .reset()
            .append_markdown("**Test**")
            .reset()
            .append_tool_output("Final");
        assert_eq!(content, AccumulatedContent::ToolOutput("Final".to_string()));
        assert_eq!(content.into_text(), Some("Final".to_string()));
    }
}
