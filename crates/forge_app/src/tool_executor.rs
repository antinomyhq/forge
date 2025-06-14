use std::sync::Arc;

use forge_display::TitleFormat;
use forge_domain::{ToolCallContext, ToolCallFull, ToolOutput, Tools};

use crate::error::Error;
use crate::execution_result::ExecutionResult;
use crate::fmt_input::{FormatInput, InputFormat};
use crate::fmt_output::FormatOutput;
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

    async fn call_internal(&self, input: Tools) -> anyhow::Result<ExecutionResult> {
        Ok(match input {
            Tools::ForgeToolFsRead(input) => self
                .services
                .fs_read_service()
                .read(input.path.clone(), input.start_line, input.end_line)
                .await?
                .into(),
            Tools::ForgeToolFsCreate(input) => self
                .services
                .fs_create_service()
                .create(input.path.clone(), input.content, input.overwrite, true)
                .await?
                .into(),
            Tools::ForgeToolFsSearch(input) => self
                .services
                .fs_search_service()
                .search(
                    input.path.clone(),
                    input.regex.clone(),
                    input.file_pattern.clone(),
                )
                .await?
                .into(),
            Tools::ForgeToolFsRemove(input) => self
                .services
                .fs_remove_service()
                .remove(input.path.clone())
                .await?
                .into(),
            Tools::ForgeToolFsPatch(input) => self
                .services
                .fs_patch_service()
                .patch(
                    input.path.clone(),
                    input.search,
                    input.operation,
                    input.content,
                )
                .await?
                .into(),
            Tools::ForgeToolFsUndo(input) => self
                .services
                .fs_undo_service()
                .undo(input.path)
                .await?
                .into(),
            Tools::ForgeToolProcessShell(input) => self
                .services
                .shell_service()
                .execute(input.command, input.cwd, input.keep_ansi)
                .await?
                .into(),
            Tools::ForgeToolNetFetch(input) => self
                .services
                .net_fetch_service()
                .fetch(input.url.clone(), input.raw)
                .await?
                .into(),
            Tools::ForgeToolFollowup(input) => self
                .services
                .follow_up_service()
                .follow_up(
                    input.question,
                    input
                        .option1
                        .into_iter()
                        .chain(input.option2.into_iter())
                        .chain(input.option3.into_iter())
                        .chain(input.option4.into_iter())
                        .chain(input.option5.into_iter())
                        .collect(),
                    input.multiple,
                )
                .await?
                .into(),
            Tools::ForgeToolAttemptCompletion(_input) => {
                crate::execution_result::ExecutionResult::AttemptCompletion
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
            // Send failure message
            context
                .send_text(TitleFormat::error(error.to_string()))
                .await?;
        }

        let execution_result = execution_result?;

        // Send formatted output message
        if let Some(output) = execution_result.to_content(&env) {
            context.send_text(output).await?;
        }

        let truncation_path = execution_result
            .to_create_temp(self.services.as_ref())
            .await?;

        Ok(execution_result.into_tool_output(tool_input, truncation_path, &env))
    }
}
