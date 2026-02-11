use std::collections::HashMap;
use std::sync::Arc;

use derive_setters::Setters;
use forge_domain::{
    Agent, Conversation, Environment, ExtensionStat, File, Model, SystemContext, Template,
    ToolDefinition, ToolUsagePrompt,
};
use tracing::debug;

use crate::{ShellService, SkillFetchService, TemplateEngine};

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

impl<S: SkillFetchService + ShellService> SystemPrompt<S> {
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

    /// Fetches file extension statistics by running git ls-files command.
    ///
    /// Uses only `git ls-files` (cross-platform) and performs counting and
    /// sorting in Rust to avoid platform-specific shell utilities like `awk`
    /// and `sort -rn`.
    async fn fetch_extensions(&self) -> anyhow::Result<Vec<ExtensionStat>> {
        let cwd = self.environment.cwd.clone();

        let output = self
            .services
            .execute("git ls-files".to_string(), cwd, false, true, None, None)
            .await?;

        // If git command fails (e.g., not in a git repo), return empty stats
        if output.output.exit_code != Some(0) {
            return Ok(vec![]);
        }

        let mut counts = output
            .output
            .stdout
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .fold(HashMap::<&str, usize>::new(), |mut acc, line| {
                let file_name = line.rsplit_once(['/', '\\']).map_or(line, |(_, f)| f);
                match file_name.rsplit_once('.') {
                    Some((prefix, ext)) if !prefix.is_empty() => {
                        *acc.entry(ext).or_default() += 1;
                        acc
                    }
                    _ => acc,
                }
            })
            .into_iter()
            .map(|(extension, count)| ExtensionStat { extension: extension.to_string(), count })
            .collect::<Vec<_>>();

        counts.sort_by(|a, b| b.count.cmp(&a.count));

        Ok(counts)
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

            // Fetch extension statistics from git
            let extensions = self.fetch_extensions().await.unwrap_or_default();

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
                extensions,
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
    use std::path::PathBuf;
    use std::sync::Arc;

    use fake::Fake;
    use forge_domain::{Agent, Environment};

    use super::*;
    use crate::ShellOutput;

    #[derive(derive_setters::Setters)]
    struct MockSkillFetchService {
        shell_output: ShellOutput,
    }

    impl Default for MockSkillFetchService {
        fn default() -> Self {
            Self {
                shell_output: ShellOutput {
                    output: forge_domain::CommandOutput {
                        stdout: String::new(),
                        stderr: String::new(),
                        command: String::new(),
                        exit_code: Some(1),
                    },
                    shell: "/bin/bash".to_string(),
                    description: None,
                },
            }
        }
    }

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

    #[async_trait::async_trait]
    impl ShellService for MockSkillFetchService {
        async fn execute(
            &self,
            _command: String,
            _cwd: PathBuf,
            _keep_ansi: bool,
            _silent: bool,
            _env_vars: Option<Vec<String>>,
            _description: Option<String>,
        ) -> anyhow::Result<ShellOutput> {
            Ok(self.shell_output.clone())
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
        let services = Arc::new(MockSkillFetchService::default());
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
    async fn test_fetch_extensions_parses_and_sorts_git_output() {
        use pretty_assertions::assert_eq;

        // Fixture
        let shell_output = ShellOutput {
            output: forge_domain::CommandOutput {
                stdout: "src/main.rs\nsrc/lib.rs\ntests/test1.rs\nREADME.md\ndocs/guide.md\nCargo.toml\nsrc/utils.rs\n".to_string(),
                stderr: String::new(),
                command: "git ls-files".to_string(),
                exit_code: Some(0),
            },
            shell: "/bin/bash".to_string(),
            description: None,
        };
        let services = Arc::new(MockSkillFetchService::default().shell_output(shell_output));
        let env = create_test_environment();
        let agent = create_test_agent();
        let system_prompt = SystemPrompt::new(services, env, agent);

        // Actual
        let actual = system_prompt.fetch_extensions().await.unwrap();

        // Expected - sorted by count descending
        let expected = vec![
            ExtensionStat { extension: "rs".to_string(), count: 4 },
            ExtensionStat { extension: "md".to_string(), count: 2 },
            ExtensionStat { extension: "toml".to_string(), count: 1 },
        ];

        assert_eq!(actual, expected);
    }
}
