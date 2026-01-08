use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use console::style;
use forge_domain::{
    Agent, AgentId, AgentInput, ChatResponse, ChatResponseContent, Environment, Model,
    SystemContext, ToolCallContext, ToolCallFull, ToolCatalog, ToolDefinition, ToolName,
    ToolOutput, ToolResult,
};
use forge_template::Element;
use futures::future::join_all;
use strum::IntoEnumIterator;
use tokio::time::timeout;

use crate::agent_executor::AgentExecutor;
use crate::dto::ToolsOverview;
use crate::error::Error;
use crate::fmt::content::FormatContent;
use crate::mcp_executor::McpExecutor;
use crate::tool_executor::ToolExecutor;
use crate::{
    EnvironmentService, McpService, PolicyService, Services, ToolResolver, WorkspaceService,
};

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

    /// Check if a tool operation is allowed based on the workflow policies
    async fn check_tool_permission(
        &self,
        tool_input: &ToolCatalog,
        context: &ToolCallContext,
    ) -> anyhow::Result<bool> {
        let cwd = self.services.get_environment().cwd;
        let operation = tool_input.to_policy_operation(cwd.clone());
        if let Some(operation) = operation {
            let decision = self.services.check_operation_permission(&operation).await?;

            // Send custom policy message to the user when a policy file was created
            if let Some(policy_path) = decision.path {
                use forge_domain::TitleFormat;

                use crate::utils::format_display_path;
                context
                    .send_title(
                        TitleFormat::debug("Permissions Update")
                            .sub_title(format_display_path(policy_path.as_path(), &cwd)),
                    )
                    .await?;
            }
            if !decision.allowed {
                return Ok(true);
            }
        }
        Ok(false)
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
            let tool_input: ToolCatalog = ToolCatalog::try_from(input)?;
            let env = self.services.get_environment();
            if let Some(content) = tool_input.to_content(&env) {
                context.send(content).await?;
            }

            // Check permissions before executing the tool (only in restricted mode)
            // This is done BEFORE the timeout to ensure permissions are never timed out
            if self.services.is_restricted()
                && self.check_tool_permission(&tool_input, context).await?
            {
                // Send formatted output message for policy denial
                context
                    .send(forge_domain::TitleFormat::error("Permission Denied"))
                    .await?;

                return Ok(ToolOutput::text(
                    Element::new("permission_denied")
                        .cdata("User has denied the permission to execute this tool"),
                ));
            }

            self.call_with_timeout(&tool_name, || {
                self.tool_executor.execute(tool_input, context)
            })
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

    pub async fn tools_overview(&self, model: Option<Model>) -> anyhow::Result<ToolsOverview> {
        let mcp_tools = self.services.get_mcp_servers().await?;
        let agent_tools = self.agent_executor.agent_definitions().await?;

        // Check if current working directory is indexed
        let environment = self.services.get_environment();
        let cwd = environment.cwd.clone();
        let is_indexed = self.services.is_indexed(&cwd).await.unwrap_or(false);
        let is_authenticated = self.services.is_authenticated().await.unwrap_or(false);

        Ok(ToolsOverview::new()
            .system(Self::get_system_tools(
                is_indexed && is_authenticated,
                &environment,
                model,
            ))
            .agents(agent_tools)
            .mcp(mcp_tools))
    }
}

impl<S> ToolRegistry<S> {
    fn get_system_tools(
        sem_search_supported: bool,
        env: &Environment,
        model: Option<Model>,
    ) -> Vec<ToolDefinition> {
        use crate::TemplateEngine;

        let handlebars = TemplateEngine::handlebar_instance();

        // Create template data with environment nested under "env"
        let ctx = SystemContext { env: Some(env.clone()), model, ..Default::default() };

        ToolCatalog::iter()
            .filter(|tool| {
                // Filter out sem_search if cwd is not indexed
                if matches!(tool, ToolCatalog::SemSearch(_)) {
                    sem_search_supported
                } else {
                    true
                }
            })
            .map(|tool| {
                let mut def = tool.definition();
                // Render template variables in description
                if let Ok(rendered) = handlebars.render_template(&def.description, &ctx) {
                    def.description = rendered;
                }
                def
            })
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
    use forge_domain::{Agent, AgentId, Environment, ModelId, ProviderId, ToolCatalog, ToolName};
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
        use fake::{Fake, Faker};
        let env: Environment = Faker.fake();
        let actual = ToolRegistry::<()>::get_system_tools(true, &env, None);
        assert!(actual.iter().any(|t| t.name.as_str() == "sem_search"));
    }

    #[test]
    fn test_sem_search_filtered_when_not_supported() {
        use fake::{Fake, Faker};
        let env: Environment = Faker.fake();
        let actual = ToolRegistry::<()>::get_system_tools(false, &env, None);
        assert!(actual.iter().all(|t| t.name.as_str() != "sem_search"));
    }
}

#[cfg(test)]
fn create_test_model(
    id: &str,
    modalities: Vec<forge_domain::InputModality>,
) -> forge_domain::Model {
    use forge_domain::{Model, ModelId};

    Model {
        id: ModelId::new(id),
        name: Some(format!("Test {}", id)),
        description: None,
        context_length: Some(128000),
        tools_supported: Some(true),
        supports_parallel_tool_calls: Some(true),
        supports_reasoning: Some(false),
        input_modalities: modalities,
    }
}

#[test]
fn test_template_rendering_in_tool_descriptions() {
    use fake::{Fake, Faker};

    let mut env: Environment = Faker.fake();
    env.max_search_lines = 1000;

    let actual = ToolRegistry::<()>::get_system_tools(true, &env, None);
    let fs_search_tool = actual
        .iter()
        .find(|t| t.name.as_str() == "fs_search")
        .unwrap();

    // The description should have the template variable rendered
    assert!(
        fs_search_tool.description.contains("1000"),
        "Description should contain the rendered max_search_lines value: {}",
        fs_search_tool.description
    );
    assert!(
        !fs_search_tool
            .description
            .contains("{{env.maxSearchLines}}"),
        "Description should not contain unrendered template variable"
    );
}

#[test]
fn test_dynamic_tool_description_with_vision_model() {
    use fake::{Fake, Faker};
    use forge_domain::InputModality;

    let env: Environment = Faker.fake();
    let vision_model = create_test_model("gpt-4o", vec![InputModality::Text, InputModality::Image]);

    let tools_with_vision = ToolRegistry::<()>::get_system_tools(true, &env, Some(vision_model));
    let read_tool = tools_with_vision
        .iter()
        .find(|t| t.name.as_str() == "read")
        .unwrap();

    // Vision-capable model should see image and PDF support
    assert!(
        read_tool.description.contains("**Images**"),
        "Vision model should see image support in description"
    );
    assert!(
        read_tool.description.contains("**PDFs**"),
        "Vision model should see PDF support in description"
    );
}

#[test]
fn test_dynamic_tool_description_with_text_only_model() {
    use fake::{Fake, Faker};
    use forge_domain::InputModality;

    let env: Environment = Faker.fake();
    let text_only_model = create_test_model("gpt-3.5-turbo", vec![InputModality::Text]);

    let tools_text_only = ToolRegistry::<()>::get_system_tools(true, &env, Some(text_only_model));
    let read_tool = tools_text_only
        .iter()
        .find(|t| t.name.as_str() == "read")
        .unwrap();

    // Text-only model should NOT see image and PDF support
    assert!(
        !read_tool.description.contains("**Images**"),
        "Text-only model should not see image support in description"
    );
    assert!(
        !read_tool.description.contains("**PDFs**"),
        "Text-only model should not see PDF support in description"
    );
    // Should still see text file support
    assert!(
        read_tool.description.contains("**Text files**"),
        "Text-only model should see text file support"
    );
}

#[test]
fn test_dynamic_tool_description_without_model() {
    use fake::{Fake, Faker};

    let env: Environment = Faker.fake();

    // When no model is provided, should default to showing minimal capabilities
    let tools_no_model = ToolRegistry::<()>::get_system_tools(true, &env, None);
    let read_tool = tools_no_model
        .iter()
        .find(|t| t.name.as_str() == "read")
        .unwrap();

    // Without model info, should show basic text file support
    assert!(
        read_tool.description.contains("**Text files**"),
        "Should show text file support even without model"
    );
}
