use std::sync::Arc;

use derive_setters::Setters;
use forge_domain::{
    Agent, Conversation, Environment, Model, Skill, SystemContext, Template, ToolDefinition,
    ToolUsagePrompt,
};
use tracing::debug;

use crate::TemplateService;

#[derive(Setters)]
pub struct SystemPrompt<S> {
    services: Arc<S>,
    environment: Environment,
    agent: Agent,
    tool_definitions: Vec<ToolDefinition>,
    files: Vec<String>,
    models: Vec<Model>,
    custom_instructions: Vec<String>,
    skills: Vec<Skill>,
}

impl<S: TemplateService> SystemPrompt<S> {
    pub fn new(services: Arc<S>, environment: Environment, agent: Agent) -> Self {
        Self {
            services,
            environment,
            agent,
            models: Vec::default(),
            tool_definitions: Vec::default(),
            files: Vec::default(),
            custom_instructions: Vec::default(),
            skills: Vec::default(),
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
            let mut files = self.files.clone();
            files.sort();

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

            let ctx = SystemContext {
                env: Some(env),
                tool_information,
                tool_supported,
                files,
                custom_rules: custom_rules.join("\n\n"),
                supports_parallel_tool_calls,
                skills: self.skills.clone(),
            };

            let static_block = self
                .services
                .render_template(Template::new(&system_prompt.template), &())
                .await?;
            let non_static_block = self
                .services
                .render_template(Template::new("{{> forge-custom-agent-template.md }}"), &ctx)
                .await?;

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
    use std::path::PathBuf;
    use std::sync::Arc;

    use fake::Fake;
    use forge_domain::{Agent, Environment, Skill};

    use super::*;

    struct MockTemplateService;

    #[async_trait::async_trait]
    impl TemplateService for MockTemplateService {
        async fn render_template<T: serde::Serialize + Sync + Send>(
            &self,
            _template: Template<T>,
            _context: &T,
        ) -> anyhow::Result<String> {
            Ok(String::new())
        }

        async fn register_template(&self, _path: PathBuf) -> anyhow::Result<()> {
            Ok(())
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
            ProviderId::Forge,
            ModelId::new("test_model"),
        )
    }

    #[tokio::test]
    async fn test_system_prompt_with_skills() {
        // Fixture
        let services = Arc::new(MockTemplateService);
        let env = create_test_environment();
        let agent = create_test_agent();
        let skills = vec![
            Skill::new(
                "code_review",
                "/skills/code_review.md",
                "Review code",
                "Code review skill",
            ),
            Skill::new(
                "testing",
                "/skills/testing.md",
                "Write tests",
                "Testing skill",
            ),
        ];

        let system_prompt = SystemPrompt::new(services, env, agent).skills(skills.clone());

        // Act - create a conversation and add system message
        let conversation = forge_domain::Conversation::generate();
        let result = system_prompt.add_system_message(conversation).await;

        // Assert
        assert!(result.is_ok());
        let conversation = result.unwrap();
        assert!(conversation.context.is_some());
    }
}
