use std::path::PathBuf;
use std::sync::Arc;

use forge_domain::{LineNumbers, TitleFormat, ToolCallContext, ToolCallFull, ToolOutput, Tools};

use crate::fmt::content::FormatContent;
use crate::operation::{TempContentFiles, ToolOperation};
use crate::services::ShellService;
use crate::utils::format_display_path;
use crate::{
    ConversationService, EnvironmentService, FollowUpService, FsCreateService, FsPatchService,
    FsReadService, FsRemoveService, FsSearchService, FsUndoService, ImageReadService,
    NetFetchService, PlanCreateService, PolicyService,
};

pub struct ToolExecutor<S> {
    services: Arc<S>,
}

impl<
    S: FsReadService
        + ImageReadService
        + FsCreateService
        + FsSearchService
        + NetFetchService
        + FsRemoveService
        + FsPatchService
        + FsUndoService
        + ShellService
        + FollowUpService
        + ConversationService
        + EnvironmentService
        + PlanCreateService
        + PolicyService,
> ToolExecutor<S>
{
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }

    /// Check if a tool operation is allowed based on the workflow policies
    #[allow(unused)]
    async fn check_tool_permission(
        &self,
        tool_input: &Tools,
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
                false,
            )
            .await?;
        Ok(path)
    }

    async fn call_internal(&self, input: Tools) -> anyhow::Result<ToolOperation> {
        Ok(match input {
            Tools::Read(input) => {
                let normalized_path = self.normalize_path(input.path.clone());
                let output = self
                    .services
                    .read(
                        normalized_path,
                        input.start_line.map(|i| i as u64),
                        input.end_line.map(|i| i as u64),
                    )
                    .await?;
                let output = if input.show_line_numbers {
                    let file_content = output.content.file_content();
                    let numbered_content = file_content.numbered_from(output.start_line as usize);
                    output.content(crate::Content::file(numbered_content))
                } else {
                    output
                };

                (input, output).into()
            }
            Tools::ReadImage(input) => {
                let output = self.services.read_image(input.path.clone()).await?;
                output.into()
            }
            Tools::Write(input) => {
                let normalized_path = self.normalize_path(input.path.clone());
                let output = self
                    .services
                    .create(
                        normalized_path,
                        input.content.clone(),
                        input.overwrite,
                        true,
                    )
                    .await?;
                (input, output).into()
            }
            Tools::Search(input) => {
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
            Tools::Remove(input) => {
                let normalized_path = self.normalize_path(input.path.clone());
                let output = self.services.remove(normalized_path).await?;
                (input, output).into()
            }
            Tools::Patch(input) => {
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
            Tools::Undo(input) => {
                let normalized_path = self.normalize_path(input.path.clone());
                let output = self.services.undo(normalized_path).await?;
                (input, output).into()
            }
            Tools::Shell(input) => {
                let normalized_cwd = self.normalize_path(input.cwd.display().to_string());
                let output = self
                    .services
                    .execute(
                        input.command.clone(),
                        PathBuf::from(normalized_cwd),
                        input.keep_ansi,
                        input.env.clone(),
                    )
                    .await?;
                output.into()
            }
            Tools::Fetch(input) => {
                let output = self.services.fetch(input.url.clone(), input.raw).await?;
                (input, output).into()
            }
            Tools::Followup(input) => {
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
            Tools::Plan(input) => {
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
        })
    }

    pub async fn execute(
        &self,
        input: ToolCallFull,
        context: &ToolCallContext,
    ) -> anyhow::Result<ToolOutput> {
        let tool_name = input.name.clone();
        let tool_input: Tools = Tools::try_from(input)?;
        let env = self.services.get_environment();
        if let Some(content) = tool_input.to_content(&env) {
            context.send(content).await?;
        }

        // Check permissions before executing the tool
        // if self.check_tool_permission(&tool_input, context).await? {
        //     // Send formatted output message for policy denial

        //     context
        //         .send(ContentFormat::from(TitleFormat::error("Permission Denied")))
        //         .await?;

        //     return Ok(ToolOutput::text(
        //         Element::new("permission_denied")
        //             .cdata("User has denied the permission to execute this tool"),
        //     ));
        // }

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
            operation.into_tool_output(tool_name, truncation_path, &env, metrics)
        })
    }
}
