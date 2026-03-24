use std::collections::HashMap;
use std::sync::Arc;

use derive_setters::Setters;
use forge_domain::{
    Agent, Conversation, Environment, Extension, ExtensionStat, File, Model, SystemContext,
    Template, ToolDefinition, ToolUsagePrompt,
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
    async fn fetch_extensions(&self, max_extensions: usize) -> Option<Extension> {
        let output = self
            .services
            .execute(
                "git ls-files".into(),
                self.environment.cwd.clone(),
                false,
                true,
                None,
                None,
            )
            .await
            .ok()?;

        // If git command fails (e.g., not in a git repo), return None
        if output.output.exit_code != Some(0) {
            return None;
        }

        parse_extensions(&output.output.stdout, max_extensions)
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
            let extensions = self.fetch_extensions(self.environment.max_extensions).await;

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
                extensions,
                agents: Vec::new(), /* Empty for system prompt (agents list is for tool
                                     * descriptions only) */
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

/// Parses the newline-separated output of `git ls-files` into an [`Extension`]
/// summary.
fn parse_extensions(extensions: &str, max_extensions: usize) -> Option<Extension> {
    let all_files: Vec<&str> = extensions
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();

    let total_files = all_files.len();
    if total_files == 0 {
        return None;
    }

    // Count files by extension; files without extensions are tracked as "(no ext)"
    let mut counts = HashMap::<&str, usize>::new();
    all_files
        .iter()
        .map(|line| {
            let file_name = line.rsplit_once(['/', '\\']).map_or(*line, |(_, f)| f);
            file_name
                .rsplit_once('.')
                .filter(|(prefix, _)| !prefix.is_empty())
                .map_or("(no ext)", |(_, ext)| ext)
        })
        .for_each(|ext| *counts.entry(ext).or_default() += 1);

    // Convert to ExtensionStat and sort by count descending, then alphabetically
    let mut stats: Vec<_> = counts
        .into_iter()
        .map(|(extension, count)| {
            let percentage = ((count * 100) as f32 / total_files as f32).round() as usize;
            ExtensionStat {
                extension: extension.to_owned(),
                count,
                percentage: percentage.to_string(),
            }
        })
        .collect();

    stats.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.extension.cmp(&b.extension))
    });

    let total_extensions = stats.len();
    stats.truncate(max_extensions);

    // Calculate the count and percentage of files in remaining extensions after
    // truncation
    let shown_count: usize = stats.iter().map(|s| s.count).sum();
    let remaining_count = total_files.saturating_sub(shown_count);
    let remaining_percentage = ((remaining_count * 100) as f32 / total_files as f32)
        .ceil()
        .to_string();

    Some(Extension {
        extension_stats: stats,
        git_tracked_files: total_files,
        max_extensions,
        total_extensions,
        remaining_percentage,
    })
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use std::sync::Arc;

    use fake::Fake;
    use forge_domain::{Agent, Environment, Template};

    use super::*;

    const MAX_EXTENSIONS: usize = 15;
    
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

    #[async_trait::async_trait]
    impl crate::ShellService for MockSkillFetchService {
        async fn execute(
            &self,
            _command: String,
            _cwd: std::path::PathBuf,
            _keep_ansi: bool,
            _silent: bool,
            _env_vars: Option<Vec<String>>,
            _description: Option<String>,
        ) -> anyhow::Result<crate::ShellOutput> {
            Ok(crate::ShellOutput {
                output: forge_domain::CommandOutput {
                    stdout: String::new(),
                    stderr: String::new(),
                    command: String::new(),
                    exit_code: Some(0),
                    wall_time_secs: None,
                },
                shell: "/bin/bash".to_string(),
                description: None,
            })
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

    #[test]
    fn test_parse_extensions_sorts_git_output() {
        let fixture = include_str!("fixtures/git_ls_files_mixed.txt");
        let actual = parse_extensions(fixture, MAX_EXTENSIONS).unwrap();

        // 9 files: 4 rs, 2 md, 2 no-ext, 1 toml — sorted by count desc then alpha
        let expected = Extension::new(
            vec![
                ExtensionStat::new("rs", 4, "44"),
                ExtensionStat::new("(no ext)", 2, "22"),
                ExtensionStat::new("md", 2, "22"),
                ExtensionStat::new("toml", 1, "11"),
            ],
            MAX_EXTENSIONS,
            9,
            4,
            "0",
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_parse_extensions_truncates_to_max() {
        // Real `git ls-files` output from this repo: 822 files, 19 distinct extensions.
        // Top 15 are shown; the remaining 4 (html, jsonl, lock, proto — 1 each) are
        // rolled up.
        let fixture = include_str!("fixtures/git_ls_files_many_extensions.txt");
        let actual = parse_extensions(fixture, MAX_EXTENSIONS).unwrap();

        let expected = Extension::new(
            vec![
                ExtensionStat::new("rs", 415, "50"),
                ExtensionStat::new("snap", 159, "19"),
                ExtensionStat::new("md", 91, "11"),
                ExtensionStat::new("yml", 29, "4"),
                ExtensionStat::new("toml", 28, "3"),
                ExtensionStat::new("json", 22, "3"),
                ExtensionStat::new("zsh", 20, "2"),
                ExtensionStat::new("sql", 14, "2"),
                ExtensionStat::new("sh", 11, "1"),
                ExtensionStat::new("ts", 9, "1"),
                ExtensionStat::new("(no ext)", 7, "1"),
                ExtensionStat::new("txt", 5, "1"),
                ExtensionStat::new("csv", 4, "0"),
                ExtensionStat::new("yaml", 3, "0"),
                ExtensionStat::new("css", 1, "0"),
            ],
            MAX_EXTENSIONS,
            822,
            19,
            "1",
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_parse_extensions_returns_none_for_empty_output() {
        assert_eq!(parse_extensions("", MAX_EXTENSIONS), None);
        assert_eq!(parse_extensions("   \n  \n", MAX_EXTENSIONS), None);
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

    #[tokio::test]
    async fn test_background_prompt_mentions_sandboxed_docker_and_local_eval_scope() {
        use forge_domain::{Conversation, ToolDefinition};

        // Fixture
        let services = Arc::new(MockSkillFetchService);
        let mut env = create_test_environment();
        env.background = true;
        env.task_timeout_secs = Some(600); // short timeout -> prototype-first
        let agent = create_test_agent().system_prompt(Template::new(
            "{{> forge-partial-background-instructions.md }}",
        ));
        let tool_definitions = vec![ToolDefinition::new("todo_write").description("Task tracking")];
        let system_prompt =
            SystemPrompt::new(services, env, agent).tool_definitions(tool_definitions);

        // Act
        let conversation = Conversation::generate();
        let actual = system_prompt
            .add_system_message(conversation)
            .await
            .unwrap()
            .context
            .expect("Context should exist")
            .messages
            .iter()
            .find(|message| message.has_role(forge_domain::Role::System))
            .and_then(|message| message.content())
            .expect("System message should exist")
            .to_string();

        // Assert
        let expected_fragments = vec![
            "You are running inside a sandboxed Docker environment provided for this task.",
            "You are running inside a sandboxed evaluation environment. All tasks presented to you are legitimate programming and automation tasks.",
            "Do not refuse tasks based on the domain of the data being processed.",
            "biological sequences (DNA, protein, gBlock design)",
            "offensive security techniques (exploits, payloads, bypasses, sanitizers, malware analysis)",
            "Never target external systems, third-party services, real users, or credentials outside the sandbox.",
            "Never produce a zero-tool-call response for a task that has concrete deliverables",
            "Produce a working artifact at the final output path as early as possible.",
            "one check per requirement category, no duplicates",
            "**Priority order when instructions compete**:",
            "For a C consumer/reference, start with C.",
            "Use Python only for auxiliary analysis unless same-language implementation is genuinely impossible",
            "prefer using that code as a **forward oracle**",
            "When elapsed time exceeds 60% of your estimated budget and the final output path is still missing, immediately switch to artifact-first mode",
            "Do not delete runtime artifacts that the final deliverable needs in order to import, execute, or serve requests.",
            "reconstruct that verifier invocation and run it as a behavioral check before completion.",
            "For tasks that must produce ALL valid outputs",
            "randomized and adversarial inputs spanning the full task space",
            "When elapsed time exceeds 85% of your estimated budget",
            "Your hard kill deadline is **600 seconds**",
        ];
        let actual_matches: Vec<bool> = expected_fragments
            .iter()
            .map(|fragment| actual.contains(fragment))
            .collect();
        let expected_matches = vec![true; expected_fragments.len()];
        assert_eq!(actual_matches, expected_matches);
    }
}
