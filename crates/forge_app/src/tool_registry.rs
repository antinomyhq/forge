use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use console::style;
use forge_domain::{
    Agent, AgentId, AgentInput, ChatResponse, ChatResponseContent, ToolCallContext, ToolCallFull,
    ToolCatalog, ToolDefinition, ToolName, ToolOutput, ToolResult,
};
use futures::future::join_all;
use strum::IntoEnumIterator;
use tokio::time::timeout;

use crate::agent_executor::AgentExecutor;
use crate::dto::ToolsOverview;
use crate::error::Error;
use crate::mcp_executor::McpExecutor;
use crate::tool_executor::ToolExecutor;
use crate::{ContextEngineService, EnvironmentService, McpService, Services, ToolResolver};

pub struct ToolRegistry<S> {
    tool_executor: ToolExecutor<S>,
    agent_executor: AgentExecutor<S>,
    mcp_executor: McpExecutor<S>,
    tool_timeout: Duration,
    services: Arc<S>,
}

impl<S: Services> ToolRegistry<S> {
    pub fn new(services: Arc<S>) -> Self {
        Self {
            services: services.clone(),
            tool_executor: ToolExecutor::new(services.clone()),
            agent_executor: AgentExecutor::new(services.clone()),
            mcp_executor: McpExecutor::new(services.clone()),
            tool_timeout: Duration::from_secs(services.get_environment().tool_timeout),
        }
    }

    async fn call_with_timeout<F, Fut>(
        &self,
        tool_name: &ToolName,
        future: F,
    ) -> anyhow::Result<ToolOutput>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<ToolOutput>>,
    {
        timeout(self.tool_timeout, future())
            .await
            .context(Error::CallTimeout {
                timeout: self.tool_timeout.as_secs() / 60,
                tool_name: tool_name.clone(),
            })?
    }

    async fn call_inner(
        &self,
        agent: &Agent,
        input: ToolCallFull,
        context: &ToolCallContext,
    ) -> anyhow::Result<ToolOutput> {
        Self::validate_tool_call(agent, &input.name)?;

        tracing::info!(tool_name = %input.name, arguments = %input.arguments.clone().into_string(), "Executing tool call");
        let tool_name = input.name.clone();

        // First, try to call a Forge tool
        if ToolCatalog::contains(&input.name) {
            self.call_with_timeout(&tool_name, || self.tool_executor.execute(input, context))
                .await
        } else if self.agent_executor.contains_tool(&input.name).await? {
            // Handle agent delegation tool calls
            let agent_input = AgentInput::try_from(&input)?;
            let executor = self.agent_executor.clone();
            // NOTE: Agents should not timeout
            let outputs =
                join_all(agent_input.tasks.into_iter().map(|task| {
                    executor.execute(AgentId::new(input.name.as_str()), task, context)
                }))
                .await
                .into_iter()
                .collect::<anyhow::Result<Vec<_>>>()?;
            Ok(ToolOutput::from(outputs.into_iter()))
        } else if self.mcp_executor.contains_tool(&input.name).await? {
            let output = self
                .call_with_timeout(&tool_name, || self.mcp_executor.execute(input, context))
                .await?;
            let text = output
                .values
                .iter()
                .filter_map(|output| output.as_str())
                .fold(String::new(), |mut a, b| {
                    a.push('\n');
                    a.push_str(b);
                    a
                });
            if !text.trim().is_empty() {
                let text = style(text).cyan().dim().to_string();
                context
                    .send(ChatResponse::TaskMessage {
                        content: ChatResponseContent::PlainText(text),
                    })
                    .await?;
            }
            Ok(output)
        } else {
            Err(Error::NotFound(input.name).into())
        }
    }

    pub async fn call(
        &self,
        agent: &Agent,
        context: &ToolCallContext,
        call: ToolCallFull,
    ) -> ToolResult {
        let call_id = call.call_id.clone();
        let tool_name = call.name.clone();
        let output = self.call_inner(agent, call, context).await;

        ToolResult::new(tool_name).call_id(call_id).output(output)
    }

    pub async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        Ok(self.tools_overview().await?.into())
    }
    pub async fn tools_overview(&self) -> anyhow::Result<ToolsOverview> {
        let mcp_tools = self.services.get_mcp_servers().await?;
        let agent_tools = self.agent_executor.agent_definitions().await?;

        // Check if current working directory is indexed
        let cwd = self.services.get_environment().cwd.clone();
        let is_indexed = self.services.is_indexed(&cwd).await.unwrap_or(false);
        let is_authenticated = self.services.is_authenticated().await.unwrap_or(false);

        Ok(ToolsOverview::new()
            .system(Self::get_system_tools(is_indexed && is_authenticated))
            .agents(agent_tools)
            .mcp(mcp_tools))
    }
}

impl<S> ToolRegistry<S> {
    fn get_system_tools(sem_search_supported: bool) -> Vec<ToolDefinition> {
        ToolCatalog::iter()
            .filter(|tool| {
                // Filter out sem_search if cwd is not indexed
                if matches!(tool, ToolCatalog::SemSearch(_)) {
                    sem_search_supported
                } else {
                    true
                }
            })
            .map(|tool| tool.definition())
            .collect::<Vec<_>>()
    }

    /// Validates if a tool is supported by both the agent and the system.
    ///
    /// # Validation Process
    /// Verifies the tool is supported by the agent specified in the context
    fn validate_tool_call(agent: &Agent, tool_name: &ToolName) -> Result<(), Error> {
        // Check if tool matches any pattern (supports globs like "mcp_*")
        let matches = ToolResolver::is_allowed(agent, tool_name);
        if !matches {
            tracing::error!(tool_name = %tool_name, "No tool with name");
            let supported_tools = agent
                .tools
                .iter()
                .flatten()
                .map(|t| t.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(Error::NotAllowed { name: tool_name.clone(), supported_tools });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{Agent, AgentId, ModelId, ProviderId, ToolCatalog, ToolName};
    use pretty_assertions::assert_eq;

    use crate::error::Error;
    use crate::tool_registry::ToolRegistry;

    fn agent() -> Agent {
        // only allow read and search tools for this agent
        Agent::new(
            AgentId::new("test_agent"),
            ProviderId::ANTHROPIC,
            ModelId::new("claude-3-5-sonnet-20241022"),
        )
        .tools(vec![ToolName::new("read"), ToolName::new("fs_search")])
    }

    #[tokio::test]
    async fn test_restricted_tool_call() {
        let result = ToolRegistry::<()>::validate_tool_call(
            &agent(),
            &ToolName::new(ToolCatalog::Read(Default::default())),
        );
        assert!(result.is_ok(), "Tool call should be valid");
    }

    #[tokio::test]
    async fn test_restricted_tool_call_err() {
        let error = ToolRegistry::<()>::validate_tool_call(&agent(), &ToolName::new("write"))
            .unwrap_err()
            .to_string();
        assert_eq!(
            error,
            "Tool 'write' is not available. Please try again with one of these tools: [read, fs_search]"
        );
    }

    #[test]
    fn test_validate_tool_call_with_glob_pattern_wildcard() {
        let fixture = Agent::new(
            AgentId::new("test_agent"),
            ProviderId::ANTHROPIC,
            ModelId::new("claude-3-5-sonnet-20241022"),
        )
        .tools(vec![ToolName::new("mcp_*"), ToolName::new("read")]);

        let actual = ToolRegistry::<()>::validate_tool_call(&fixture, &ToolName::new("mcp_foo"));

        assert!(actual.is_ok());
    }

    #[test]
    fn test_validate_tool_call_with_glob_pattern_multiple_tools() {
        let fixture = Agent::new(
            AgentId::new("test_agent"),
            ProviderId::ANTHROPIC,
            ModelId::new("claude-3-5-sonnet-20241022"),
        )
        .tools(vec![ToolName::new("mcp_*"), ToolName::new("read")]);

        let actual_mcp_read =
            ToolRegistry::<()>::validate_tool_call(&fixture, &ToolName::new("mcp_read"));
        let actual_mcp_write =
            ToolRegistry::<()>::validate_tool_call(&fixture, &ToolName::new("mcp_write"));
        let actual_read = ToolRegistry::<()>::validate_tool_call(&fixture, &ToolName::new("read"));

        assert!(actual_mcp_read.is_ok());
        assert!(actual_mcp_write.is_ok());
        assert!(actual_read.is_ok());
    }

    #[test]
    fn test_validate_tool_call_with_glob_pattern_no_match() {
        let fixture = Agent::new(
            AgentId::new("test_agent"),
            ProviderId::ANTHROPIC,
            ModelId::new("claude-3-5-sonnet-20241022"),
        )
        .tools(vec![ToolName::new("mcp_*"), ToolName::new("read")]);

        let actual = ToolRegistry::<()>::validate_tool_call(&fixture, &ToolName::new("write"));

        let expected = Error::NotAllowed {
            name: ToolName::new("write"),
            supported_tools: "mcp_*, read".to_string(),
        }
        .to_string();

        assert_eq!(actual.unwrap_err().to_string(), expected);
    }

    #[test]
    fn test_validate_tool_call_with_glob_pattern_question_mark() {
        let fixture = Agent::new(
            AgentId::new("test_agent"),
            ProviderId::ANTHROPIC,
            ModelId::new("claude-3-5-sonnet-20241022"),
        )
        .tools(vec![ToolName::new("read?"), ToolName::new("write")]);

        let actual_read1 =
            ToolRegistry::<()>::validate_tool_call(&fixture, &ToolName::new("read1"));
        let actual_readx =
            ToolRegistry::<()>::validate_tool_call(&fixture, &ToolName::new("readx"));
        let actual_read = ToolRegistry::<()>::validate_tool_call(&fixture, &ToolName::new("read"));

        assert!(actual_read1.is_ok());
        assert!(actual_readx.is_ok());
        assert!(actual_read.is_err());
    }

    #[test]
    fn test_validate_tool_call_with_glob_pattern_character_class() {
        let fixture = Agent::new(
            AgentId::new("test_agent"),
            ProviderId::ANTHROPIC,
            ModelId::new("claude-3-5-sonnet-20241022"),
        )
        .tools(vec![ToolName::new("tool_[abc]"), ToolName::new("write")]);

        let actual_tool_a =
            ToolRegistry::<()>::validate_tool_call(&fixture, &ToolName::new("tool_a"));
        let actual_tool_b =
            ToolRegistry::<()>::validate_tool_call(&fixture, &ToolName::new("tool_b"));
        let actual_tool_c =
            ToolRegistry::<()>::validate_tool_call(&fixture, &ToolName::new("tool_c"));
        let actual_tool_d =
            ToolRegistry::<()>::validate_tool_call(&fixture, &ToolName::new("tool_d"));

        assert!(actual_tool_a.is_ok());
        assert!(actual_tool_b.is_ok());
        assert!(actual_tool_c.is_ok());
        assert!(actual_tool_d.is_err());
    }

    #[test]
    fn test_validate_tool_call_with_glob_pattern_double_wildcard() {
        let fixture = Agent::new(
            AgentId::new("test_agent"),
            ProviderId::ANTHROPIC,
            ModelId::new("claude-3-5-sonnet-20241022"),
        )
        .tools(vec![ToolName::new("**"), ToolName::new("read")]);

        let actual_any_tool =
            ToolRegistry::<()>::validate_tool_call(&fixture, &ToolName::new("any_tool_name"));
        let actual_nested =
            ToolRegistry::<()>::validate_tool_call(&fixture, &ToolName::new("nested/tool"));

        assert!(actual_any_tool.is_ok());
        assert!(actual_nested.is_ok());
    }

    #[test]
    fn test_validate_tool_call_exact_match_with_special_chars() {
        let fixture = Agent::new(
            AgentId::new("test_agent"),
            ProviderId::ANTHROPIC,
            ModelId::new("claude-3-5-sonnet-20241022"),
        )
        .tools(vec![ToolName::new("tool_[special]"), ToolName::new("read")]);

        let actual =
            ToolRegistry::<()>::validate_tool_call(&fixture, &ToolName::new("tool_[special]"));

        // The glob pattern "tool_[special]" will match "tool_s", "tool_p", etc., not
        // the literal string So this test verifies that exact matching doesn't
        // work when the pattern is a valid glob
        assert!(actual.is_err());
    }

    #[test]
    fn test_validate_tool_call_backward_compatibility_exact_match() {
        let fixture = Agent::new(
            AgentId::new("test_agent"),
            ProviderId::ANTHROPIC,
            ModelId::new("claude-3-5-sonnet-20241022"),
        )
        .tools(vec![
            ToolName::new("read"),
            ToolName::new("write"),
            ToolName::new("fs_search"),
        ]);

        let actual_read = ToolRegistry::<()>::validate_tool_call(&fixture, &ToolName::new("read"));
        let actual_write =
            ToolRegistry::<()>::validate_tool_call(&fixture, &ToolName::new("write"));
        let actual_invalid =
            ToolRegistry::<()>::validate_tool_call(&fixture, &ToolName::new("delete"));

        assert!(actual_read.is_ok());
        assert!(actual_write.is_ok());
        assert!(actual_invalid.is_err());
    }

    #[test]
    fn test_validate_tool_call_empty_tools_list() {
        let fixture = Agent::new(
            AgentId::new("test_agent"),
            ProviderId::ANTHROPIC,
            ModelId::new("claude-3-5-sonnet-20241022"),
        );

        let actual = ToolRegistry::<()>::validate_tool_call(&fixture, &ToolName::new("read"));

        assert!(actual.is_err());
    }

    #[test]
    fn test_validate_tool_call_glob_with_prefix_suffix() {
        let fixture = Agent::new(
            AgentId::new("test_agent"),
            ProviderId::ANTHROPIC,
            ModelId::new("claude-3-5-sonnet-20241022"),
        )
        .tools(vec![ToolName::new("mcp_*_tool")]);

        let actual_match =
            ToolRegistry::<()>::validate_tool_call(&fixture, &ToolName::new("mcp_read_tool"));
        let actual_no_match =
            ToolRegistry::<()>::validate_tool_call(&fixture, &ToolName::new("mcp_read"));

        assert!(actual_match.is_ok());
        assert!(actual_no_match.is_err());
    }

    #[test]
    fn test_sem_search_included_when_supported() {
        let actual = ToolRegistry::<()>::get_system_tools(true);
        assert!(actual.iter().any(|t| t.name.as_str() == "sem_search"));
    }

    #[test]
    fn test_sem_search_filtered_when_not_supported() {
        let actual = ToolRegistry::<()>::get_system_tools(false);
        assert!(actual.iter().all(|t| t.name.as_str() != "sem_search"));
    }
}
