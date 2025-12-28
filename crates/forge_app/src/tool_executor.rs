use std::path::PathBuf;
use std::sync::Arc;

use forge_domain::{
    CodebaseQueryResult, FSMultiPatch, TitleFormat, ToolCallContext, ToolCallFull, ToolCatalog,
    ToolOutput,
};
use forge_template::Element;

use crate::fmt::content::FormatContent;
use crate::operation::{TempContentFiles, ToolOperation};
use crate::services::ShellService;
use crate::utils::format_display_path;
use crate::{
    ConversationService, EnvironmentService, FollowUpService, FsCreateService, FsMultiPatchService,
    FsPatchService, FsReadService, FsRemoveService, FsSearchService, FsUndoService,
    ImageReadService, NetFetchService, PlanCreateService, PolicyService, SkillFetchService,
    WorkspaceService,
};

pub struct ToolExecutor<S> {
    services: Arc<S>,
}

impl<
    S: FsReadService
        + ImageReadService
        + FsCreateService
        + FsSearchService
        + WorkspaceService
        + NetFetchService
        + FsRemoveService
        + FsPatchService
        + FsMultiPatchService
        + FsUndoService
        + ShellService
        + FollowUpService
        + ConversationService
        + EnvironmentService
        + PlanCreateService
        + PolicyService
        + SkillFetchService,
> ToolExecutor<S>
{
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
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

    async fn dump_operation(&self, operation: &ToolOperation) -> anyhow::Result<TempContentFiles> {
        match operation {
            ToolOperation::NetFetch { input: _, output } => {
                let original_length = output.content.len();
                let is_truncated =
                    original_length > self.services.get_environment().fetch_truncation_limit;
                let mut files = TempContentFiles::default();

                if is_truncated {
                    files = files.stdout(
                        self.create_temp_file("forge_fetch_", ".txt", &output.content)
                            .await?,
                    );
                }

                Ok(files)
            }
            ToolOperation::Shell { output } => {
                let env = self.services.get_environment();
                let stdout_lines = output.output.stdout.lines().count();
                let stderr_lines = output.output.stderr.lines().count();
                let stdout_truncated =
                    stdout_lines > env.stdout_max_prefix_length + env.stdout_max_suffix_length;
                let stderr_truncated =
                    stderr_lines > env.stdout_max_prefix_length + env.stdout_max_suffix_length;

                let mut files = TempContentFiles::default();

                if stdout_truncated {
                    files = files.stdout(
                        self.create_temp_file("forge_shell_stdout_", ".txt", &output.output.stdout)
                            .await?,
                    );
                }
                if stderr_truncated {
                    files = files.stderr(
                        self.create_temp_file("forge_shell_stderr_", ".txt", &output.output.stderr)
                            .await?,
                    );
                }

                Ok(files)
            }
            _ => Ok(TempContentFiles::default()),
        }
    }

    /// Converts a path to absolute by joining it with the current working
    /// directory if it's relative
    fn normalize_path(&self, path: String) -> String {
        let env = self.services.get_environment();
        let path_buf = PathBuf::from(&path);

        if path_buf.is_absolute() {
            path
        } else {
            PathBuf::from(&env.cwd).join(path_buf).display().to_string()
        }
    }

    async fn create_temp_file(
        &self,
        prefix: &str,
        ext: &str,
        content: &str,
    ) -> anyhow::Result<std::path::PathBuf> {
        let path = tempfile::Builder::new()
            .disable_cleanup(true)
            .prefix(prefix)
            .suffix(ext)
            .tempfile()?
            .into_temp_path()
            .to_path_buf();
        self.services
            .create(
                path.to_string_lossy().to_string(),
                content.to_string(),
                true,
            )
            .await?;
        Ok(path)
    }

    async fn call_internal(&self, input: ToolCatalog) -> anyhow::Result<ToolOperation> {
        Ok(match input {
            ToolCatalog::Read(input) => {
                let normalized_path = self.normalize_path(input.path.clone());
                let output = self
                    .services
                    .read(
                        normalized_path,
                        input.start_line.map(|i| i as u64),
                        input.end_line.map(|i| i as u64),
                    )
                    .await?;

                (input, output).into()
            }
            ToolCatalog::ReadImage(input) => {
                let normalized_path = self.normalize_path(input.path.clone());
                let output = self.services.read_image(normalized_path).await?;
                output.into()
            }
            ToolCatalog::Write(input) => {
                let normalized_path = self.normalize_path(input.path.clone());
                let output = self
                    .services
                    .create(normalized_path, input.content.clone(), input.overwrite)
                    .await?;
                (input, output).into()
            }
            ToolCatalog::FsSearch(input) => {
                let normalized_path = self.normalize_path(input.path.clone());
                let output = self
                    .services
                    .search(
                        normalized_path,
                        input.regex.clone(),
                        input.file_pattern.clone(),
                    )
                    .await?;
                (input, output).into()
            }
            ToolCatalog::SemSearch(input) => {
                let env = self.services.get_environment();
                let services = self.services.clone();
                let cwd = env.cwd.clone();
                let limit = env.sem_search_limit;
                let top_k = env.sem_search_top_k as u32;
                let params: Vec<_> = input
                    .queries
                    .iter()
                    .map(|search_query| {
                        let mut params = forge_domain::SearchParams::new(
                            &search_query.query,
                            &search_query.use_case,
                        )
                        .limit(limit)
                        .top_k(top_k);
                        if let Some(ext) = &input.file_extension {
                            params = params.ends_with(ext);
                        }
                        params
                    })
                    .collect();

                // Execute all queries in parallel
                let futures: Vec<_> = params
                    .into_iter()
                    .map(|param| services.query_workspace(cwd.clone(), param))
                    .collect();

                let mut results = futures::future::try_join_all(futures).await?;

                // Deduplicate results across queries
                crate::search_dedup::deduplicate_results(&mut results);

                let output = input
                    .queries
                    .into_iter()
                    .zip(results.into_iter())
                    .map(|(query, results)| CodebaseQueryResult {
                        query: query.query,
                        use_case: query.use_case,
                        results,
                    })
                    .collect::<Vec<_>>();

                let output = forge_domain::CodebaseSearchResults { queries: output };
                ToolOperation::CodebaseSearch { output }
            }
            ToolCatalog::Remove(input) => {
                let normalized_path = self.normalize_path(input.path.clone());
                let output = self.services.remove(normalized_path).await?;
                (input, output).into()
            }
            ToolCatalog::Patch(input) => {
                let normalized_path = self.normalize_path(input.path.clone());
                let output = self
                    .services
                    .patch(
                        normalized_path,
                        input.search.clone(),
                        input.operation.clone(),
                        input.content.clone(),
                    )
                    .await?;
                (input, output).into()
            }
            ToolCatalog::MultiPatch(input) => {
                let normalized_path = self.normalize_path(input.path.clone());
                let patches = FSMultiPatch {
                    path: normalized_path,
                    edits: input.edits.clone(),
                };
                let output = self.services.multi_patch(patches).await?;
                (input, output).into()
            }
            ToolCatalog::Undo(input) => {
                let normalized_path = self.normalize_path(input.path.clone());
                let output = self.services.undo(normalized_path).await?;
                (input, output).into()
            }
            ToolCatalog::Shell(input) => {
                let normalized_cwd = self.normalize_path(input.cwd.display().to_string());
                let output = self
                    .services
                    .execute(
                        input.command.clone(),
                        PathBuf::from(normalized_cwd),
                        input.keep_ansi,
                        false,
                        input.env.clone(),
                    )
                    .await?;
                output.into()
            }
            ToolCatalog::Fetch(input) => {
                let output = self.services.fetch(input.url.clone(), input.raw).await?;
                (input, output).into()
            }
            ToolCatalog::Followup(input) => {
                let output = self
                    .services
                    .follow_up(
                        input.question.clone(),
                        input
                            .option1
                            .clone()
                            .into_iter()
                            .chain(input.option2.clone().into_iter())
                            .chain(input.option3.clone().into_iter())
                            .chain(input.option4.clone().into_iter())
                            .chain(input.option5.clone().into_iter())
                            .collect(),
                        input.multiple,
                    )
                    .await?;
                output.into()
            }
            ToolCatalog::Plan(input) => {
                let output = self
                    .services
                    .create_plan(
                        input.plan_name.clone(),
                        input.version.clone(),
                        input.content.clone(),
                    )
                    .await?;
                (input, output).into()
            }
            ToolCatalog::Skill(input) => {
                let skill = self.services.fetch_skill(input.name.clone()).await?;
                (input, skill).into()
            }
        })
    }

    pub async fn execute(
        &self,
        input: ToolCallFull,
        context: &ToolCallContext,
    ) -> anyhow::Result<ToolOutput> {
        let tool_input: ToolCatalog = ToolCatalog::try_from(input)?;
        let tool_kind = tool_input.kind();
        let env = self.services.get_environment();
        if let Some(content) = tool_input.to_content(&env) {
            context.send(content).await?;
        }

        // Check permissions before executing the tool (if enabled)
        if env.enable_permissions && self.check_tool_permission(&tool_input, context).await? {
            // Send formatted output message for policy denial
            context
                .send(TitleFormat::error("Permission Denied"))
                .await?;

            return Ok(ToolOutput::text(
                Element::new("permission_denied")
                    .cdata("User has denied the permission to execute this tool"),
            ));
        }

        let execution_result = self.call_internal(tool_input.clone()).await;

        if let Err(ref error) = execution_result {
            tracing::error!(error = ?error, "Tool execution failed");
        }

        let operation = execution_result?;

        // Send formatted output message
        if let Some(output) = operation.to_content(&env) {
            context.send(output).await?;
        }

        let truncation_path = self.dump_operation(&operation).await?;

        context.with_metrics(|metrics| {
            operation.into_tool_output(tool_kind, truncation_path, &env, metrics)
        })
    }
}
