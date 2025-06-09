use std::cmp::min;
use std::fmt::Write;
use std::sync::Arc;

use forge_display::{GrepFormat, TitleFormat};
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

    #[allow(dead_code)]
    async fn call_internal(
        &self,
        arguments: Value,
        context: &mut ToolCallContext,
    ) -> anyhow::Result<ToolOutput> {
        let input = serde_json::from_value::<ToolInput>(arguments)?;
        match input {
            ToolInput::FSRead(input) => {
                let output = self
                    .services
                    .fs_read_service()
                    .read(input.path, input.start_line, input.end_line)
                    .await?;
                
                send_read_context(context, &output).await?;
                
                Ok(ToolOutput::text(format_fs_read(output)?))
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
                let output = self
                    .services
                    .fs_search_service()
                    .search(
                        input.path.clone(),
                        input.regex.clone(),
                        input.file_pattern.clone(),
                    )
                    .await?;
                if let Some(output) = output.as_ref() {
                    context.send_text(&output.title).await?;
                    let mut formatted_output = GrepFormat::new(output.matches.clone());
                    if let Some(regex) = &output.regex {
                        formatted_output = formatted_output.regex(regex.clone());
                    }
                    context.send_text(formatted_output.format()).await?;
                }

                let output =
                    format_fs_search(output, &input.path, &input.regex, &input.file_pattern)
                        .await?;

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
    pub async fn call(&self, _input: ToolCallFull, _context: &mut ToolCallContext) -> ToolResult {
        unimplemented!()
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

async fn format_fs_search(
    search_output: Option<SearchResult>,
    path: &str,
    regex: &Option<String>,
    file_pattern: &Option<String>,
) -> anyhow::Result<String> {
    match search_output {
        None => Ok("No matches found.".to_string()),
        Some(output) => {
            let mut result = String::new();
            writeln!(result, "---")?;
            writeln!(result, "path: {}", path)?;
            if let Some(regex) = regex {
                writeln!(result, "regex: {}", regex)?;
            }
            if let Some(file_pattern) = file_pattern {
                writeln!(result, "file_pattern: {}", file_pattern)?;
            }
            writeln!(result, "total_lines: {}", output.total_lines)?;
            writeln!(result, "start_line: 1")?;
            writeln!(
                result,
                "end_line: {}",
                output.total_lines.min(output.max_lines)
            )?;

            if let Some(path) = output.truncation_path {
                writeln!(result, "temp_file: {}", path.display())?;
                let truncation_tag = format!(
                    "\n<truncation>content is truncated to {} lines, remaining content can be read from path:{}</truncation>",
                    output.max_lines,
                    path.display()
                );
                result.push_str(&truncation_tag);
            }

            Ok(result)
        }
    }
}

async fn send_read_context(ctx: &mut ToolCallContext, out: &ReadOutput) -> anyhow::Result<()> {
    let is_range_relevant = out.is_explicit_range || out.is_truncated;
    // Set the title based on whether this was an explicit user range request
    // or an automatic limit for large files that actually needed truncation
    let title = if out.is_explicit_range {
        "Read (Range)"
    } else if out.is_truncated {
        // Only show "Auto-Limited" if the file was actually truncated
        "Read (Auto-Limited)"
    } else {
        // File was smaller than the limit, so no truncation occurred
        "Read"
    };
    let end_info = min(out.end_line, out.total_lines);
    let range_info = format!(
        "line range: {}-{}, total lines: {}",
        out.start_line, end_info, out.total_lines
    );
    // Build the subtitle conditionally using a string buffer
    let mut subtitle = String::new();

    // Always include the file path
    subtitle.push_str(&out.display_path);

    // Add range info if relevant
    if is_range_relevant {
        // Add range info for explicit ranges or truncated files
        subtitle.push_str(&format!(" ({range_info})"));
    }
    let message = TitleFormat::debug(title).sub_title(subtitle);
    ctx.send_text(message).await?;
    Ok(())
}

fn format_fs_read(out: ReadOutput) -> anyhow::Result<String> {
    let mut result = String::new();

    writeln!(result, "---")?;
    writeln!(result, "path: {}", out.path)?;
    // Determine if range information is relevant to display
    let is_range_relevant = out.is_explicit_range || out.is_truncated;
    
    if is_range_relevant {
        writeln!(result, "start_line: {}", out.start_line)?;
        writeln!(result, "end_line: {}", out.end_line)?;
        writeln!(result, "total_lines: {}", out.total_lines)?;
    }
    
    writeln!(result, "---")?;
    writeln!(result, "{}", out.content)?;
    
    Ok(result)
}
