use std::sync::Arc;

use derive_setters::Setters;
use forge_domain::{
    Agent, Conversation, Environment, File, Model, SystemContext, Template, ToolDefinition,
    ToolUsagePrompt,
};
use tracing::debug;

use crate::{SkillFetchService, TemplateEngine};

#[derive(Setters)]
pub struct SystemPrompt<S> {
    services: Arc<S>,
    environment: Environment,
    agent: Agent,
    tool_definitions: Vec<ToolDefinition>,
    files: Vec<File>,
    models: Vec<Model>,
    custom_instructions: Vec<String>,
}

impl<S: SkillFetchService> SystemPrompt<S> {
    pub fn new(services: Arc<S>, environment: Environment, agent: Agent) -> Self {
        Self {
            services,
            environment,
            agent,
            models: Vec::default(),
            tool_definitions: Vec::default(),
            files: Vec::default(),
            custom_instructions: Vec::default(),
        }
    }

    pub async fn add_system_message(
        &self,
        mut conversation: Conversation,
    ) -> anyhow::Result<Conversation> {
        let context = conversation.context.take().unwrap_or_default();
        let agent = &self.agent;
        let context = if let Some(system_prompt) = &agent.system_prompt {
            let env = self.environment.clone();
            let files = self.files.clone();

            let tool_supported = self.is_tool_supported()?;
            let supports_parallel_tool_calls = self.is_parallel_tool_call_supported();
            let tool_information = match tool_supported {
                true => None,
                false => Some(ToolUsagePrompt::from(&self.tool_definitions).to_string()),
            };

            let mut custom_rules = Vec::new();

            agent.custom_rules.iter().for_each(|rule| {
                custom_rules.push(rule.as_str());
            });

            self.custom_instructions.iter().for_each(|rule| {
                custom_rules.push(rule.as_str());
            });

            let skills = self.services.list_skills().await?;

            // Populate tool_names map for template rendering
            let tool_names = self
                .tool_definitions
                .iter()
                .map(|tool| {
                    (
                        tool.name.to_string(),
                        serde_json::Value::String(tool.name.to_string()),
                    )
                })
                .collect();

            let ctx = SystemContext {
                env: Some(env),
                tool_information,
                tool_supported,
                files,
                custom_rules: custom_rules.join("\n\n"),
                supports_parallel_tool_calls,
                skills,
                model: None,
                tool_names,
                agents: Vec::new(), /* Empty for system prompt (agents list is for tool
                                     * descriptions only) */
            };

            let static_block = TemplateEngine::default()
                .render_template(Template::new(&system_prompt.template), &ctx)?;

            context.set_system_messages(vec![static_block])
        } else {
            context
        };

        Ok(conversation.context(context))
    }

    // Returns if agent supports tool or not.
    fn is_tool_supported(&self) -> anyhow::Result<bool> {
        let agent = &self.agent;
        let model_id = &agent.model;

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

    /// Checks if parallel tool calls is supported by agent
    fn is_parallel_tool_call_supported(&self) -> bool {
        let agent = &self.agent;
        self.models
            .iter()
            .find(|model| model.id == agent.model)
            .and_then(|model| model.supports_parallel_tool_calls)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use fake::Fake;
    use forge_domain::{Agent, Environment};

    use super::*;

    struct MockSkillFetchService;

    #[async_trait::async_trait]
    impl SkillFetchService for MockSkillFetchService {
        async fn fetch_skill(&self, _skill_name: String) -> anyhow::Result<forge_domain::Skill> {
            Ok(
                forge_domain::Skill::new("test_skill", "Test skill", "Test skill description")
                    .path("/skills/test.md"),
            )
        }

        async fn list_skills(&self) -> anyhow::Result<Vec<forge_domain::Skill>> {
            Ok(vec![])
        }
    }

    fn create_test_environment() -> Environment {
        use fake::Faker;
        Faker.fake()
    }

    fn create_test_agent() -> Agent {
        use forge_domain::{AgentId, ModelId, ProviderId};
        Agent::new(
            AgentId::new("test_agent"),
            ProviderId::FORGE,
            ModelId::new("test_model"),
        )
    }

    #[tokio::test]
    async fn test_system_prompt_adds_context() {
        // Fixture
        let services = Arc::new(MockSkillFetchService);
        let env = create_test_environment();
        let agent = create_test_agent();
        let system_prompt = SystemPrompt::new(services, env, agent);

        // Act - create a conversation and add system message
        let conversation = forge_domain::Conversation::generate();
        let result = system_prompt.add_system_message(conversation).await;

        // Assert
        assert!(result.is_ok());
        let conversation = result.unwrap();
        assert!(conversation.context.is_some());
    }

    #[tokio::test]
    async fn test_tool_names_populated_in_context() {
        use forge_domain::{Template, ToolDefinition};

        // Fixture - create system prompt with tool definitions
        let services = Arc::new(MockSkillFetchService);
        let env = create_test_environment();
        let agent = create_test_agent().system_prompt(Template::new(
            "Tools: {{tool_names.todo_write}}, {{tool_names.read}}",
        ));

        let tool_definitions = vec![
            ToolDefinition::new("todo_write").description("Task tracking"),
            ToolDefinition::new("read").description("Read files"),
            ToolDefinition::new("write").description("Write files"),
        ];

        let system_prompt =
            SystemPrompt::new(services, env, agent).tool_definitions(tool_definitions);

        // Act
        let conversation = forge_domain::Conversation::generate();
        let result = system_prompt.add_system_message(conversation).await;

        // Assert - verify tool_names are available in rendered template
        assert!(result.is_ok());
        let conversation = result.unwrap();
        let context = conversation.context.expect("Context should exist");
        let system_message = context
            .messages
            .iter()
            .find(|m| m.has_role(forge_domain::Role::System))
            .expect("System message should exist");

        let content = system_message.content().expect("Content should exist");

        // Verify template variables were resolved
        assert!(
            content.contains("Tools: todo_write, read"),
            "Template should resolve {{{{tool_names.todo_write}}}} and {{{{tool_names.read}}}}, got: {}",
            content
        );
    }

    #[tokio::test]
    async fn test_conditional_tool_names_when_tool_missing() {
        use forge_domain::{Template, ToolDefinition};

        // Fixture - create system prompt with conditional tool reference
        let services = Arc::new(MockSkillFetchService);
        let env = create_test_environment();
        let agent = create_test_agent().system_prompt(Template::new(
            "Search using {{#if tool_names.sem_search}}{{tool_names.sem_search}}, {{/if}}{{tool_names.fs_search}}",
        ));

        // Only include fs_search, not sem_search
        let tool_definitions = vec![ToolDefinition::new("fs_search").description("File search")];

        let system_prompt =
            SystemPrompt::new(services, env, agent).tool_definitions(tool_definitions);

        // Act
        let conversation = forge_domain::Conversation::generate();
        let result = system_prompt.add_system_message(conversation).await;

        // Assert - verify conditional rendering works when tool is missing
        assert!(result.is_ok());
        let conversation = result.unwrap();
        let context = conversation.context.expect("Context should exist");
        let system_message = context
            .messages
            .iter()
            .find(|m| m.has_role(forge_domain::Role::System))
            .expect("System message should exist");

        let content = system_message.content().expect("Content should exist");

        // Should render only fs_search since sem_search is not available
        assert!(
            content.contains("Search using fs_search"),
            "Template should conditionally omit sem_search, got: {}",
            content
        );
        // Should not have double commas or extra spaces from missing tool
        assert!(
            !content.contains("Search using , fs_search"),
            "Template should not have empty tool reference, got: {}",
            content
        );
    }

    #[tokio::test]
    async fn test_conditional_tool_names_when_tool_present() {
        use forge_domain::{Template, ToolDefinition};

        // Fixture - create system prompt with conditional tool reference
        let services = Arc::new(MockSkillFetchService);
        let env = create_test_environment();
        let agent = create_test_agent().system_prompt(Template::new(
            "Search using {{#if tool_names.sem_search}}{{tool_names.sem_search}}, {{/if}}{{tool_names.fs_search}}",
        ));

        // Include both tools
        let tool_definitions = vec![
            ToolDefinition::new("sem_search").description("Semantic search"),
            ToolDefinition::new("fs_search").description("File search"),
        ];

        let system_prompt =
            SystemPrompt::new(services, env, agent).tool_definitions(tool_definitions);

        // Act
        let conversation = forge_domain::Conversation::generate();
        let result = system_prompt.add_system_message(conversation).await;

        // Assert - verify conditional rendering includes both tools
        assert!(result.is_ok());
        let conversation = result.unwrap();
        let context = conversation.context.expect("Context should exist");
        let system_message = context
            .messages
            .iter()
            .find(|m| m.has_role(forge_domain::Role::System))
            .expect("System message should exist");

        let content = system_message.content().expect("Content should exist");

        // Should render both tools
        assert!(
            content.contains("Search using sem_search, fs_search"),
            "Template should include both tools, got: {}",
            content
        );
    }
}
