use std::sync::Arc;

use anyhow::Context;
use forge_domain::{ModelId, TaskList, ToolCallContext, ToolCallFull, ToolOutput, Tools};
use futures::StreamExt;

use crate::error::Error;
use crate::fmt::content::FormatContent;
use crate::operation::Operation;
use crate::services::ShellService;
use crate::{
    ConversationService, EnvironmentService, FollowUpService, FsCreateService, FsPatchService,
    FsReadService, FsRemoveService, FsSearchService, FsUndoService, IndexCodebaseService,
    NetFetchService, ProviderService,
};

/// System prompt used for codebase search
const CODEBASE_SEARCH: &str = include_str!("../../../templates/sage-agent.hbs");

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
        + EnvironmentService
        + IndexCodebaseService
        + ProviderService,
> ToolExecutor<S>
{
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
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
            Tools::ForgeToolCodebaseSearch(input) => {
                let index_output = self.services.index(input.paths.clone()).await?;

                // Process shards in parallel
                let shard_futures: Vec<_> = index_output
                    .shards
                    .into_iter()
                    .map(|shard| {
                        let services = self.services.clone();
                        let query = input.query.clone();
                        async move {
                            let context = forge_domain::Context::default()
                                .add_message(forge_domain::ContextMessage::system(CODEBASE_SEARCH))
                                .add_message(forge_domain::ContextMessage::user(
                                    format!("<codebase>{}</codebase>\n query: {}", shard.0, query),
                                    None,
                                ));

                            let mut response = services
                                .chat(&ModelId::new("google/gemini-2.5-pro"), context)
                                .await?;

                            let mut shard_result = String::new();
                            while let Some(message) = response.next().await {
                                let message = message?;
                                if let Some(content) = message.content {
                                    shard_result.push_str(content.as_str());
                                }
                            }

                            anyhow::Ok(shard_result)
                        }
                    })
                    .collect();

                // Execute all shard processing in parallel and collect results
                let shard_results = futures::future::join_all(shard_futures)
                    .await
                    .into_iter()
                    .collect::<anyhow::Result<Vec<_>>>()?;

                // Filter out empty results
                let results: Vec<String> = shard_results
                    .into_iter()
                    .filter(|result| !result.is_empty())
                    .collect();

                let output = if results.is_empty() {
                    "No results found from codebase analysis.".to_string()
                } else {
                    format!(
                        "Codebase Analysis Results:\n\n{}",
                        results.join("\n\n---\n\n")
                    )
                };

                Operation::CodebaseSearch { input, output }
            }
        })
    }

    pub async fn execute(
        &self,
        input: ToolCallFull,
        context: &mut ToolCallContext,
    ) -> anyhow::Result<ToolOutput> {
        let tool_input = Tools::try_from(input).map_err(Error::CallArgument)?;
        let env = self.services.get_environment();
        if let Some(content) = tool_input.to_content(&env) {
            context.send(content).await?;
        }

        // Send tool call information

        let execution_result = self.call_internal(tool_input, &mut context.tasks).await;
        if let Err(ref error) = execution_result {
            tracing::error!(error = ?error, "Tool execution failed");
        }

        let execution_result = execution_result?;

        // Send formatted output message
        if let Some(output) = execution_result.to_content(&env) {
            context.send(output).await?;
        }

        let truncation_path = execution_result
            .to_create_temp(self.services.as_ref())
            .await?;

        Ok(execution_result.into_tool_output(truncation_path, &env))
    }
}
