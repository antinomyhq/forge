use std::fmt::Write;
use std::sync::Arc;

use forge_display::TitleFormat;
use forge_domain::{
    Tool, ToolCallContext, ToolCallFull, ToolDefinition, ToolInput, ToolName, ToolOutput,
    ToolResult,
};
use serde_json::Value;

use crate::{
    AttemptCompletionService, FetchOutput, FollowUpService, FsCreateService, FsPatchService,
    FsReadService, FsRemoveService, FsSearchService, FsUndoService, NetFetchService, PatchOutput,
    ReadOutput, SearchResult, Services, ShellOutput, ShellService,
};

pub struct ToolRegistry<S> {
    #[allow(dead_code)]
    services: Arc<S>,
}
impl<S: Services> ToolRegistry<S> {
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }
    async fn call_internal(
        &self,
        arguments: Value,
        context: &mut ToolCallContext,
    ) -> anyhow::Result<ToolOutput> {
        let input = serde_json::from_value::<ToolInput>(arguments)?;
        match input {
            ToolInput::FSRead(input) => {
                let _tool_output = format_fs_read(
                    self.services.fs_read_service().read(input.path).await?,
                    input.start_line,
                    input.end_line,
                )
                .await?;
                unimplemented!()
            }
            ToolInput::FSWrite(input) => {
                let out = self
                    .services
                    .fs_create_service()
                    .create(input.path, input.content, input.overwrite)
                    .await?;

                let mut result = String::new();

                writeln!(result, "---")?;
                writeln!(result, "path: {}", out.path)?;
                if out.exists {
                    writeln!(result, "operation: OVERWRITE")?;
                } else {
                    writeln!(result, "operation: CREATE")?;
                }
                writeln!(result, "total_chars: {}", out.chars)?;
                if let Some(warning) = out.warning {
                    writeln!(result, "Warning: {warning}")?;
                }
                writeln!(result, "---")?;

                let title = if out.exists { "Overwrite" } else { "Create" };

                context
                    .send_text(format!(
                        "{}",
                        TitleFormat::debug(title).sub_title(out.formatted_path)
                    ))
                    .await?;
                context.send_text(out.diff).await?;

                Ok(ToolOutput::text(result))
            }
            ToolInput::FSSearch(input) => {
                let output = format_fs_search(
                    self.services
                        .fs_search_service()
                        .search(input.path, input.regex, input.file_pattern)
                        .await?,
                )
                .await;

                Ok(ToolOutput::text(output))
            }
            ToolInput::FSRemove(input) => {
                let output = self.services.fs_remove_service().remove(input.path).await?;

                let message = TitleFormat::debug("Remove").sub_title(&output.display_path);

                // Send the formatted message
                context.send_text(message).await?;

                Ok(ToolOutput::text(format!(
                    "Successfully removed file: {}",
                    output.display_path
                )))
            }
            ToolInput::FSPatch(input) => {
                let output = self
                    .services
                    .fs_patch_service()
                    .patch(input.path, input.search, input.operation, input.content)
                    .await?;

                context
                    .send_text(format!(
                        "{}",
                        TitleFormat::debug("Patch").sub_title(&output.display_path)
                    ))
                    .await?;

                // Output diff either to sender or println
                context.send_text(&output.diff).await?;

                Ok(ToolOutput::text(format_fs_patch(output)?))
            }
            ToolInput::FSUndo(input) => {
                let output = self.services.fs_undo_service().undo(input.path).await?;

                // Display a message about the file being undone
                let message = TitleFormat::debug("Undo").sub_title(&output);
                context.send_text(message).await?;

                Ok(ToolOutput::text(format_fs_undo(output)))
            }
            ToolInput::Shell(input) => {
                let output = self
                    .services
                    .shell_service()
                    .execute(input.command, input.cwd, input.keep_ansi)
                    .await?;

                let title_format =
                    TitleFormat::debug(format!("Execute [{}]", output.shell.as_str()))
                        .sub_title(&output.output.command);
                context.send_text(title_format).await?;

                Ok(ToolOutput::text(format_shell(output)?))
            }
            ToolInput::NetFetch(input) => {
                let out = self
                    .services
                    .net_fetch_service()
                    .fetch(input.url, input.raw)
                    .await?;
                context
                    .send_text(
                        TitleFormat::debug(format!("GET {}", out.code)).sub_title(out.url.as_str()),
                    )
                    .await?;
                Ok(ToolOutput::text(format_net_fetch(out)?))
            }
            ToolInput::Followup(input) => {
                let output = self
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
                    .await?;

                Ok(ToolOutput::text(format_followup(output, context).await))
            }
            ToolInput::AttemptCompletion(input) => {
                let out = self
                    .services
                    .attempt_completion_service()
                    .attempt_completion(input.result)
                    .await?;
                context.send_summary(out).await?;
                context.set_complete().await;
                Ok(ToolOutput::text(
                    "[Task was completed successfully. Now wait for user feedback]".to_string(),
                ))
            }
        }
    }
    #[allow(dead_code)]
    pub async fn call(&self, input: ToolCallFull, context: &mut ToolCallContext) -> ToolResult {
        ToolResult::new(input.name)
            .call_id(input.call_id)
            .output(self.call_internal(input.arguments, context).await)
    }
    #[allow(dead_code)]
    pub async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        unimplemented!()
    }
    #[allow(dead_code)]
    pub async fn find(&self, _: &ToolName) -> anyhow::Result<Option<Arc<Tool>>> {
        unimplemented!()
    }
}

async fn format_followup(output: Option<String>, context: &mut ToolCallContext) -> String {
    match output {
        None => {
            context.set_complete().await;
            "User interrupted the selection".to_string()
        }
        Some(v) => v,
    }
}

fn format_net_fetch(out: FetchOutput) -> anyhow::Result<String> {
    let mut result = String::new();

    writeln!(result, "---")?;
    writeln!(result, "URL: {}", out.url)?;
    writeln!(result, "total_chars: {}", out.original_length)?;
    writeln!(result, "start_char: {}", out.start_char)?;
    writeln!(result, "end_char: {}", out.end_char)?;
    writeln!(result, "context: {}", out.context)?;
    if let Some(path) = out.path.as_ref() {
        writeln!(
            result,
            "truncation: Content is truncated to {} chars; Remaining content can be read from path: {}",
            out.max_length,
            path.display()
        )?;
    }

    writeln!(result, "---")?;
    // Create truncation tag only if content was actually truncated and stored in a
    // temp file
    let truncation_tag = match out.path.as_ref() {
        Some(path) if out.is_truncated => {
            format!(
                "\n<truncation>content is truncated to {} chars, remaining content can be read from path: {}</truncation>",
                out.max_length,
                path.to_string_lossy()
            )
        }
        _ => String::new(),
    };

    Ok(format!("{result}{truncation_tag}"))
}

fn format_shell(output: ShellOutput) -> anyhow::Result<String> {
    let mut result = String::new();

    writeln!(result, "---")?;
    writeln!(result, "command: {}", output.output.command)?;
    if let Some(exit_code) = output.output.exit_code {
        writeln!(result, "exit_code: {exit_code}")?;
    }

    if output.stdout_truncated {
        writeln!(
            result,
            "total_stdout_lines: {}",
            output.stdout.lines().count()
        )?;
    }

    if output.stderr_truncated {
        writeln!(
            result,
            "total_stderr_lines: {}",
            output.stderr.lines().count()
        )?;
    }

    // Combine outputs
    let mut outputs = vec![];
    if !output.stdout.is_empty() {
        outputs.push(output.stdout);
    }
    if !output.stderr.is_empty() {
        outputs.push(output.stderr);
    }

    let mut result = if outputs.is_empty() {
        format!(
            "Command {} with no output.",
            if output.output.success() {
                "executed successfully"
            } else {
                "failed"
            }
        )
    } else {
        outputs.join("\n")
    };

    writeln!(result, "---")?;
    if let Some(path) = output.path {
        result.push_str(&format!(
            "\n<truncated>content is truncated, remaining content can be read from path:{}</truncated>",
            path.display()
        ));
    }
    if output.output.success() {
        Ok(result)
    } else {
        anyhow::bail!(result)
    }
}

fn format_fs_undo(output: String) -> String {
    format!("Successfully undid last operation on path: {output}")
}

fn format_fs_patch(output: PatchOutput) -> anyhow::Result<String> {
    let mut result = String::new();

    writeln!(result, "---")?;
    writeln!(result, "path: {}", output.path)?;
    writeln!(result, "total_chars: {}", output.chars)?;

    // Check for syntax errors
    if let Some(warning) = output.warning {
        writeln!(result, "warning:{warning}")?;
    }

    writeln!(result, "---")?;

    writeln!(result, "{}", output.diff)?;
    Ok(result)
}

async fn format_fs_search(_search_output: Vec<SearchResult>) -> String {
    todo!()
}

async fn format_fs_read(
    _read_output: ReadOutput,
    _start: Option<u64>,
    _end: Option<u64>,
) -> anyhow::Result<ToolOutput> {
    todo!()
}
