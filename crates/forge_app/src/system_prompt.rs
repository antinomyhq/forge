use std::collections::HashMap;
use std::sync::Arc;

use derive_setters::Setters;
use forge_domain::{
    Agent, Conversation, Environment, Extension, ExtensionStat, File, Model, SystemContext,
    Template, ToolDefinition, ToolUsagePrompt,
};
use tracing::debug;

use crate::{ShellService, SkillFetchService, TemplateEngine};

/// Max extensions to add in system prompt.
const MAX_EXTENSIONS: usize = 15;

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

            // Fetch extension statistics from git (top 15)
            let extensions = self.fetch_extensions(MAX_EXTENSIONS).await;

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

    Some(Extension {
        extension_stats: stats,
        git_tracked_files: total_files,
        max_extensions,
        total_extensions,
    })
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_parse_extensions_sorts_git_output() {
        let stdout = "src/main.rs\nsrc/lib.rs\ntests/test1.rs\nREADME.md\ndocs/guide.md\nCargo.toml\nsrc/utils.rs\nMakefile\nLICENSE\n";
        let actual = parse_extensions(stdout, MAX_EXTENSIONS).unwrap();

        // 9 files: 4 rs, 2 md, 2 no-ext, 1 toml — sorted by count desc then alpha
        let expected = Extension {
            extension_stats: vec![
                ExtensionStat {
                    extension: "rs".to_string(),
                    count: 4,
                    percentage: "44".to_string(),
                },
                ExtensionStat {
                    extension: "(no ext)".to_string(),
                    count: 2,
                    percentage: "22".to_string(),
                },
                ExtensionStat {
                    extension: "md".to_string(),
                    count: 2,
                    percentage: "22".to_string(),
                },
                ExtensionStat {
                    extension: "toml".to_string(),
                    count: 1,
                    percentage: "11".to_string(),
                },
            ],
            max_extensions: MAX_EXTENSIONS,
            git_tracked_files: 9,
            total_extensions: 4,
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_parse_extensions_truncates_to_max() {
        // 20 distinct extensions; ext1 has 20 files, ext20 has 1 file
        let stdout: String = (1..=20)
            .flat_map(|i| (0..(21 - i)).map(move |j| format!("file{j}.ext{i}")))
            .collect::<Vec<_>>()
            .join("\n");

        let actual = parse_extensions(&stdout, MAX_EXTENSIONS).unwrap();

        let expected = Extension {
            extension_stats: {
                let mut stats: Vec<_> = (1..=15)
                    .map(|i| {
                        let count = 21 - i;
                        let percentage = ((count * 100) as f32 / 210.0).round() as usize;
                        ExtensionStat {
                            extension: format!("ext{i}"),
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
                stats
            },
            max_extensions: MAX_EXTENSIONS,
            git_tracked_files: 210,
            total_extensions: 20,
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_parse_extensions_returns_none_for_empty_output() {
        assert_eq!(parse_extensions("", MAX_EXTENSIONS), None);
        assert_eq!(parse_extensions("   \n  \n", MAX_EXTENSIONS), None);
    }
}
