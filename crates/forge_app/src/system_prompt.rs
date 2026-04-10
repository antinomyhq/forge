use std::collections::HashMap;
use std::sync::Arc;

use derive_setters::Setters;
use forge_domain::{
    Agent, Conversation, Environment, Extension, ExtensionStat, File, Model, SystemContext,
    Template, TemplateConfig, ToolCatalog, ToolDefinition, ToolUsagePrompt,
};
use serde_json::{Map, Value, json};
use strum::IntoEnumIterator;
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
    /// Maximum number of file extensions shown in the workspace summary.
    max_extensions: usize,
    /// Configuration values passed into tool description templates.
    template_config: TemplateConfig,
}

impl<S: SkillFetchService + ShellService + crate::FileDiscoveryService> SystemPrompt<S> {
    pub fn new(services: Arc<S>, environment: Environment, agent: Agent) -> Self {
        Self {
            services,
            environment,
            agent,
            models: Vec::default(),
            tool_definitions: Vec::default(),
            files: Vec::default(),
            custom_instructions: Vec::default(),
            max_extensions: 0,
            template_config: TemplateConfig::default(),
        }
    }

    /// Fetches file extension statistics using the domain's FileDiscoveryService.
    async fn fetch_extensions(&self, max_extensions: usize) -> Option<Extension> {
        let files = self
            .services
            .as_ref()
            .collect_files(crate::Walker::unlimited().cwd(self.environment.cwd.clone()))
            .await
            .ok()?;

        if files.is_empty() {
            return None;
        }

        parse_extensions(&files, max_extensions)
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
            let extensions = self.fetch_extensions(self.max_extensions).await;

            // Build tool_names map from all available tools for template rendering
            let tool_names: Map<String, Value> = ToolCatalog::iter()
                .map(|tool| {
                    let def = tool.definition();
                    (def.name.to_string(), json!(def.name.to_string()))
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
                agents: vec![],
                config: None,
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

/// Parses a list of files into an [`Extension`] summary.
fn parse_extensions(files: &[File], max_extensions: usize) -> Option<Extension> {
    let all_files: Vec<&File> = files.iter().filter(|f| !f.is_dir).collect();
    let total_files = all_files.len();
    if total_files == 0 {
        return None;
    }

    // Count files by extension; files without extensions are tracked as "(no ext)"
    let mut counts = HashMap::<&str, usize>::new();
    all_files
        .iter()
        .map(|f| {
            let file_name = f.path.rsplit_once(['/', '\\']).map_or(f.path.as_str(), |(_, name)| name);
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

    use super::*;

    const MAX_EXTENSIONS: usize = 15;

    fn lines_to_files(lines: &str) -> Vec<File> {
        lines
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| File {
                path: line.to_string(),
                is_dir: false,
            })
            .collect()
    }


    #[test]
    fn test_parse_extensions_sorts_git_output() {
        let fixture = include_str!("fixtures/git_ls_files_mixed.txt");
        let files = lines_to_files(fixture);
        let actual = parse_extensions(&files, MAX_EXTENSIONS).unwrap();

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
        let files = lines_to_files(fixture);
        let actual = parse_extensions(&files, MAX_EXTENSIONS).unwrap();

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
        assert_eq!(parse_extensions(&[], MAX_EXTENSIONS), None);
    }
    #[derive(Clone)]
    struct MockServices {
        files_to_return: Vec<File>,
    }

    #[async_trait::async_trait]
    impl crate::FileDiscoveryService for MockServices {
        async fn collect_files(&self, _config: crate::Walker) -> anyhow::Result<Vec<File>> {
            Ok(self.files_to_return.clone())
        }
        async fn list_current_directory(&self) -> anyhow::Result<Vec<File>> {
            Ok(vec![])
        }
    }

    #[async_trait::async_trait]
    impl crate::SkillFetchService for MockServices {
        async fn fetch_skill(&self, _name: String) -> anyhow::Result<forge_domain::Skill> {
            Err(anyhow::anyhow!("not implemented"))
        }
        async fn list_skills(&self) -> anyhow::Result<Vec<forge_domain::Skill>> {
            Ok(vec![])
        }
    }

    #[async_trait::async_trait]
    impl crate::ShellService for MockServices {
        async fn execute(
            &self,
            _command: String,
            _cwd: std::path::PathBuf,
            _keep_ansi: bool,
            _silent: bool,
            _env_vars: Option<Vec<String>>,
            _description: Option<String>,
        ) -> anyhow::Result<crate::ShellOutput> {
            Ok(crate::ShellOutput { output: forge_domain::CommandOutput { command: "".into(), exit_code: Some(0), stdout: "".into(), stderr: "".into() }, shell: "sh".into(), description: None })
        }
    }

    #[tokio::test]
    async fn test_system_prompt_extension_integration() {
        let files = vec![
            forge_domain::File { path: "src/main.rs".into(), is_dir: false },
            forge_domain::File { path: "src/lib.rs".into(), is_dir: false },
            forge_domain::File { path: "README.md".into(), is_dir: false },
        ];
        
        let services = std::sync::Arc::new(MockServices { files_to_return: files });
        
        let env = forge_domain::Environment {
            os: "linux".into(),
            cwd: std::path::PathBuf::from("/tmp"),
            home: None,
            shell: "sh".into(),
            base_path: std::path::PathBuf::from("/tmp"),
        };
        
        let mut agent = forge_domain::Agent::new("test", forge_domain::ProviderId::OPENAI, forge_domain::ModelId::from("gpt-4o"));
        agent.system_prompt = Some(forge_domain::Template::new(r#"System prompt <workspace_extensions extensions="{{extensions.total_extensions}}" total="{{extensions.git_tracked_files}}" />"#));
        
        let system_prompt = SystemPrompt::new(services, env, agent).max_extensions(15);
        
        let conversation = forge_domain::Conversation::new(forge_domain::ConversationId::generate());
        let result = system_prompt.add_system_message(conversation).await.unwrap();
        
        let ctx = result.context.unwrap();
        
        let mut context_text = String::new();
        for msg in ctx.messages {
            if let forge_domain::ContextMessage::Text(text_msg) = msg.message {
                if text_msg.role == forge_domain::Role::System {
                    context_text.push_str(&text_msg.content);
                }
            }
        }
        
        // Assert extensions were fetched and integrated into context
        assert!(context_text.contains("<workspace_extensions"));
        assert!(context_text.contains(".rs"));
        assert!(context_text.contains(".md"));
    }
}