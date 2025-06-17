use std::sync::Arc;

use forge_domain::{ToolCallContext, ToolCallFull, ToolOutput, Tools};

use crate::error::Error;
use crate::fmt_input::{FormatInput, InputFormat};
use crate::fmt_output::FormatOutput;
use crate::operation::Operation;
use crate::{
    EnvironmentService, FollowUpService, FsCreateService, FsPatchService, FsReadService,
    FsRemoveService, FsSearchService, FsUndoService, NetFetchService, Services, ShellService,
};

pub struct ToolExecutor<S> {
    services: Arc<S>,
}

impl<S: Services> ToolExecutor<S> {
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }

    async fn call_internal(&self, input: Tools) -> anyhow::Result<Operation> {
        Ok(match input {
            Tools::ForgeToolFsRead(input) => {
                let output = self
                    .services
                    .fs_read_service()
                    .read(input.path.clone(), input.start_line, input.end_line)
                    .await?;
                (input, output).into()
            }
            Tools::ForgeToolFsCreate(input) => {
                let output = self
                    .services
                    .fs_create_service()
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
                    .fs_search_service()
                    .search(
                        input.path.clone(),
                        input.regex.clone(),
                        input.file_pattern.clone(),
                    )
                    .await?;
                (input, output).into()
            }
            Tools::ForgeToolFsRemove(input) => {
                let _output = self
                    .services
                    .fs_remove_service()
                    .remove(input.path.clone())
                    .await?;
                input.into()
            }
            Tools::ForgeToolFsPatch(input) => {
                let output = self
                    .services
                    .fs_patch_service()
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
                let output = self
                    .services
                    .fs_undo_service()
                    .undo(input.path.clone())
                    .await?;
                (input, output).into()
            }
            Tools::ForgeToolProcessShell(input) => {
                let output = self
                    .services
                    .shell_service()
                    .execute(input.command.clone(), input.cwd.clone(), input.keep_ansi)
                    .await?;
                output.into()
            }
            Tools::ForgeToolNetFetch(input) => {
                let output = self
                    .services
                    .net_fetch_service()
                    .fetch(input.url.clone(), input.raw)
                    .await?;
                (input, output).into()
            }
            Tools::ForgeToolFollowup(input) => {
                let output = self
                    .services
                    .follow_up_service()
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
        })
    }

    pub async fn execute(
        &self,
        input: ToolCallFull,
        context: &mut ToolCallContext,
    ) -> anyhow::Result<ToolOutput> {
        let tool_input = Tools::try_from(input).map_err(Error::CallArgument)?;
        let env = self.services.environment_service().get_environment();
        match tool_input.to_content(&env) {
            InputFormat::Title(title) => context.send_text(title).await?,
            InputFormat::Summary(summary) => context.send_summary(summary).await?,
        };

        // Send tool call information

        let execution_result = self.call_internal(tool_input.clone()).await;
        if let Err(ref error) = execution_result {
            tracing::error!(error = ?error, "Tool execution failed");
        }

        let execution_result = execution_result?;

        // Send formatted output message
        if let Some(output) = execution_result.to_content(&env) {
            context.send_text(output).await?;
        }

        let truncation_path = execution_result
            .to_create_temp(self.services.as_ref())
            .await?;

        Ok(execution_result.into_tool_output(truncation_path, &env))
    }
}
