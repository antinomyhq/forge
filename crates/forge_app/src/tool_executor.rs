use std::sync::Arc;

use anyhow::Context;
use forge_domain::{TaskList, ToolCallContext, ToolCallFull, ToolOutput, Tools};

use crate::error::Error;
use crate::fmt::content::FormatContent;
use crate::operation::{Operation, TempContentFiles};
use crate::services::ShellService;
use crate::{
    ConversationService, EnvironmentService, FollowUpService, FsCreateService, FsPatchService,
    FsReadService, FsRemoveService, FsSearchService, FsUndoService, NetFetchService,
};

pub struct ToolExecutor<S> {
    services: Arc<S>,
}

impl<
    S: FsReadService
        + FsCreateService
        + FsSearchService
        + NetFetchService
        + FsRemoveService
        + FsPatchService
        + FsUndoService
        + ShellService
        + FollowUpService
        + ConversationService
        + EnvironmentService,
> ToolExecutor<S>
{
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }
    async fn dump_operation(&self, operation: &Operation) -> anyhow::Result<TempContentFiles> {
        match operation {
            Operation::NetFetch { input: _, output } => {
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
            Operation::Shell { output } => {
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

    async fn call_internal(&self, input: Tools, tasks: &mut TaskList) -> anyhow::Result<Operation> {
        Ok(match input {
            Tools::ForgeToolFsRead(input) => {
                let output = self
                    .services
                    .read(
                        input.path.clone(),
                        input.start_line.map(|i| i as u64),
                        input.end_line.map(|i| i as u64),
                    )
                    .await?;
                (input, output).into()
            }
            Tools::ForgeToolFsCreate(input) => {
                let output = self
                    .services
                    .create(
                        input.path.clone(),
                        input.content.clone(),
                        input.overwrite,
                        true,
                    )
                    .await?;
                (input, output).into()
            }
            Tools::ForgeToolFsSearch(input) => {
                let output = self
                    .services
                    .search(
                        input.path.clone(),
                        input.regex.clone(),
                        input.file_pattern.clone(),
                    )
                    .await?;
                (input, output).into()
            }
            Tools::ForgeToolFsRemove(input) => {
                let _output = self.services.remove(input.path.clone()).await?;
                input.into()
            }
            Tools::ForgeToolFsPatch(input) => {
                let output = self
                    .services
                    .patch(
                        input.path.clone(),
                        input.search.clone(),
                        input.operation.clone(),
                        input.content.clone(),
                    )
                    .await?;
                (input, output).into()
            }
            Tools::ForgeToolFsUndo(input) => {
                let output = self.services.undo(input.path.clone()).await?;
                (input, output).into()
            }
            Tools::ForgeToolProcessShell(input) => {
                let output = self
                    .services
                    .execute(input.command.clone(), input.cwd.clone(), input.keep_ansi)
                    .await?;
                output.into()
            }
            Tools::ForgeToolNetFetch(input) => {
                let output = self.services.fetch(input.url.clone(), input.raw).await?;
                (input, output).into()
            }
            Tools::ForgeToolFollowup(input) => {
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
            Tools::ForgeToolAttemptCompletion(_input) => {
                crate::operation::Operation::AttemptCompletion
            }
            Tools::ForgeToolTaskListAppend(input) => {
                let before = tasks.clone();
                tasks.append(&input.task);
                Operation::TaskListAppend { _input: input, before, after: tasks.clone() }
            }
            Tools::ForgeToolTaskListAppendMultiple(input) => {
                let before = tasks.clone();
                tasks.append_multiple(input.tasks.clone());
                Operation::TaskListAppendMultiple { _input: input, before, after: tasks.clone() }
            }
            Tools::ForgeToolTaskListUpdate(input) => {
                let before = tasks.clone();
                tasks
                    .update_status(input.task_id, input.status.clone())
                    .context("Task not found")?;
                Operation::TaskListUpdate { _input: input, before, after: tasks.clone() }
            }
            Tools::ForgeToolTaskListList(input) => {
                let before = tasks.clone();
                // No operation needed, just return the current state
                Operation::TaskListList { _input: input, before, after: tasks.clone() }
            }
            Tools::ForgeToolTaskListClear(input) => {
                let before = tasks.clone();
                tasks.clear();
                Operation::TaskListClear { _input: input, before, after: tasks.clone() }
            }
        })
    }

    pub async fn execute(
        &self,
        input: ToolCallFull,
        context: &mut ToolCallContext,
    ) -> anyhow::Result<ToolOutput> {
        let tool_name = input.name.clone();
        let tool_input = Tools::try_from(input).map_err(Error::CallArgument)?;
        let env = self.services.get_environment();
        if let Some(content) = tool_input.to_content(&env) {
            context.send(content).await?;
        }

        // Send tool call information

        let execution_result = self
            .call_internal(tool_input.clone(), &mut context.tasks)
            .await;
        if let Err(ref error) = execution_result {
            tracing::error!(error = ?error, "Tool execution failed");
        }

        let operation = execution_result?;

        // Send formatted output message
        if let Some(output) = operation.to_content(&env) {
            context.send(output).await?;
        }

        let truncation_path = self.dump_operation(&operation).await?;

        Ok(operation.into_tool_output(tool_name, truncation_path, &env))
    }
}
