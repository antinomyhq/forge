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

            let ctx = SystemContext {
                env: Some(env),
                tool_information,
                tool_supported,
                files,
                custom_rules: custom_rules.join("\n\n"),
                supports_parallel_tool_calls,
                skills,
                model: None,
                tool_names: Default::default(),
            };

            let static_block = TemplateEngine::default()
                .render_template(Template::new(&system_prompt.template), &ctx)?;
            let non_static_block = TemplateEngine::default()
                .render_template(Template::new("{{> forge-custom-agent-template.md }}"), &ctx)?;

            context.set_system_messages(vec![static_block, non_static_block])
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
}
