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
        ctx.send_title(
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
            match message {
                ChatResponse::TaskMessage { ref content } => match content {
                    ChatResponseContent::Title(_) => ctx.send(message).await?,
                    ChatResponseContent::ToolOutput(text) => {
                        output = output.append_plain_text(text);
                    }
                    ChatResponseContent::Markdown(text) => {
                        output = output.append_markdown(text);
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
    PlainText(String),
    Markdown(String),
}

impl Default for AccumulatedContent {
    fn default() -> Self {
        Self::PlainText(String::new())
    }
}

impl AccumulatedContent {
    /// Appends plain text to the output.
    /// If currently in Markdown mode, switches to PlainText and replaces
    /// content.
    fn append_plain_text(self, text: &str) -> Self {
        match self {
            Self::PlainText(mut content) => {
                content.push_str(text);
                Self::PlainText(content)
            }
            Self::Markdown(_) => Self::PlainText(text.to_string()),
        }
    }

    /// Appends markdown to the output.
    /// If currently in PlainText mode, switches to Markdown and replaces
    /// content.
    fn append_markdown(self, text: &str) -> Self {
        match self {
            Self::Markdown(mut content) => {
                content.push_str(text);
                Self::Markdown(content)
            }
            Self::PlainText(_) => Self::Markdown(text.to_string()),
        }
    }

    /// Returns the accumulated text, or None if empty.
    fn into_text(self) -> Option<String> {
        match self {
            Self::PlainText(text) | Self::Markdown(text) if !text.is_empty() => Some(text),
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
        // Default creates empty PlainText
        let default = AccumulatedContent::default();
        assert_eq!(default, AccumulatedContent::PlainText(String::new()));

        // Empty content of both types returns None
        assert_eq!(AccumulatedContent::default().into_text(), None);
        assert_eq!(
            AccumulatedContent::PlainText(String::new()).into_text(),
            None
        );
        assert_eq!(
            AccumulatedContent::Markdown(String::new()).into_text(),
            None
        );
    }

    #[test]
    fn test_plain_text_accumulation() {
        // Single append
        let actual = AccumulatedContent::default().append_plain_text("Hello");
        assert_eq!(actual, AccumulatedContent::PlainText("Hello".to_string()));

        // Multiple appends accumulate
        let actual = AccumulatedContent::default()
            .append_plain_text("Hello")
            .append_plain_text(" ")
            .append_plain_text("World");
        assert_eq!(
            actual,
            AccumulatedContent::PlainText("Hello World".to_string())
        );

        // Non-empty content is extractable
        assert_eq!(actual.into_text(), Some("Hello World".to_string()));
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
    }

    #[test]
    fn test_mode_switching() {
        // Switching from PlainText to Markdown replaces content
        let actual = AccumulatedContent::default()
            .append_plain_text("Old text")
            .append_markdown("**New content**");
        assert_eq!(
            actual,
            AccumulatedContent::Markdown("**New content**".to_string())
        );

        // Switching from Markdown to PlainText replaces content
        let actual = AccumulatedContent::default()
            .append_markdown("**Old**")
            .append_plain_text("New content");
        assert_eq!(
            actual,
            AccumulatedContent::PlainText("New content".to_string())
        );

        // Multiple switches only keep last content
        let actual = AccumulatedContent::default()
            .append_plain_text("First")
            .append_markdown("**Second**")
            .append_plain_text("Third")
            .append_markdown("**Fourth**");
        assert_eq!(
            actual,
            AccumulatedContent::Markdown("**Fourth**".to_string())
        );
    }

    #[test]
    fn test_comprehensive_workflow() {
        // Realistic workflow: accumulate, switch, extract
        let content = AccumulatedContent::default()
            .append_plain_text("Start with plain text")
            .append_plain_text(" and continue")
            .append_markdown("\n\n**Switch to markdown**")
            .append_markdown(" with more content")
            .append_plain_text("\nBack to plain text");

        // Only the last mode's content is kept
        assert_eq!(
            content,
            AccumulatedContent::PlainText("\nBack to plain text".to_string())
        );

        // Extract the final content
        assert_eq!(
            content.into_text(),
            Some("\nBack to plain text".to_string())
        );
    }
}
