use std::cmp::min;
use std::path::Path;
use std::sync::Arc;

use forge_display::{DiffFormat, GrepFormat, TitleFormat};
use forge_domain::{
    Tool, ToolCallContext, ToolCallFull, ToolDefinition, ToolInput, ToolName, ToolOutput,
    ToolResult,
};
use serde_json::Value;

use crate::front_matter::FrontMatter;
use crate::utils::{display_path, format_display_path};
use crate::{
    AttemptCompletionService, EnvironmentService, FollowUpService, FsCreateService, FsPatchService,
    FsReadService, FsRemoveService, FsSearchService, FsUndoService, NetFetchService, PatchOutput,
    ReadOutput, Services, ShellService, TruncatedFetchOutput, TruncatedSearchOutput,
    TruncatedShellOutput, truncate_fetch_content, truncate_search_output, truncate_shell_output,
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

                let operation = if out.exists { "OVERWRITE" } else { "CREATE" };
                let metadata = FrontMatter::default()
                    .add("path", &out.path)
                    .add("operation", operation)
                    .add("total_chars", out.chars)
                    .add_optional("Warning", out.warning.as_ref());

                let title = if out.exists { "Overwrite" } else { "Create" };

                context
                    .send_text(format!(
                        "{}",
                        TitleFormat::debug(title).sub_title(out.formatted_path)
                    ))
                    .await?;
                context.send_text(out.diff).await?;

                Ok(ToolOutput::text(metadata.to_string()))
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

                let formatted_output = match output {
                    None => "No matches found.".to_string(),
                    Some(search_result) => {
                        let truncated_output = truncate_search_output(
                            &search_result.output,
                            &input.path,
                            input.regex.as_ref(),
                            input.file_pattern.as_ref(),
                        );
                        format_fs_search_truncated(truncated_output, self.services.as_ref()).await?
                    }
                };

                Ok(ToolOutput::text(formatted_output))
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
                    .patch(
                        input.path.clone(),
                        input.search,
                        input.operation,
                        input.content,
                    )
                    .await?;

                let display_path = display_path(self.services.as_ref(), Path::new(&input.path))?;
                // Generate diff between old and new content
                let diff =
                    console::strip_ansi_codes(&DiffFormat::format(&output.before, &output.after))
                        .to_string();

                context
                    .send_text(format!(
                        "{}",
                        TitleFormat::debug("Patch").sub_title(&display_path)
                    ))
                    .await?;

                // Output diff either to sender or println
                context.send_text(&diff).await?;

                Ok(ToolOutput::text(format_fs_patch(
                    &input.path,
                    output.warning,
                    diff,
                    output.after.len(),
                )?))
            }
            ToolInput::FSUndo(input) => {
                let output = self.services.fs_undo_service().undo(input.path).await?;

                // Display a message about the file being undone
                let message = TitleFormat::debug("Undo").sub_title(&output);
                context.send_text(message).await?;

                Ok(ToolOutput::text(format_fs_undo(output)))
            }
            ToolInput::Shell(input) => {
                let shell_output = self
                    .services
                    .shell_service()
                    .execute(input.command, input.cwd, input.keep_ansi)
                    .await?;

                let truncated_output = truncate_shell_output(
                    &shell_output.output.stdout,
                    &shell_output.output.stderr,
                    &shell_output.output.command,
                );

                let title_format =
                    TitleFormat::debug(format!("Execute [{}]", shell_output.shell.as_str()))
                        .sub_title(&shell_output.output.command);
                context.send_text(title_format).await?;

                Ok(ToolOutput::text(
                    format_shell_truncated(shell_output, truncated_output, self.services.as_ref())
                        .await?,
                ))
            }
            ToolInput::NetFetch(input) => {
                let fetch_output = self
                    .services
                    .net_fetch_service()
                    .fetch(input.url, input.raw)
                    .await?;

                let truncated_output = truncate_fetch_content(
                    &fetch_output.content,
                    &fetch_output.url,
                    fetch_output.code,
                    &fetch_output.context,
                );

                context
                    .send_text(
                        TitleFormat::debug(format!("GET {}", truncated_output.code))
                            .sub_title(&truncated_output.url),
                    )
                    .await?;
                Ok(ToolOutput::text(
                    format_net_fetch_truncated(truncated_output, self.services.as_ref()).await?,
                ))
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

async fn format_net_fetch_truncated<S: Services>(
    truncated_output: TruncatedFetchOutput,
    services: &S,
) -> anyhow::Result<String> {
    let mut metadata = FrontMatter::default()
        .add("URL", &truncated_output.url)
        .add("total_chars", truncated_output.original_length)
        .add("start_char", truncated_output.start_char)
        .add("end_char", truncated_output.end_char)
        .add("context", &truncated_output.context);

    // Create temp file if truncation occurred
    let path = truncated_output
        .create_temp_file_if_needed(services)
        .await?;

    if let Some(path) = path.as_ref() {
        metadata = metadata.add(
            "truncation",
            format!(
                "Content is truncated to {} chars; Remaining content can be read from path: {}",
                truncated_output.max_length,
                path.display()
            ),
        );
    }

    // Create truncation tag only if content was actually truncated and stored in a
    // temp file
    let truncation_tag = match path.as_ref() {
        Some(path) if truncated_output.is_truncated => {
            format!(
                "\n<truncation>content is truncated to {} chars, remaining content can be read from path: {}</truncation>",
                truncated_output.max_length,
                path.to_string_lossy()
            )
        }
        _ => String::new(),
    };

    Ok(format!("{metadata}{truncation_tag}"))
}

async fn format_shell_truncated<S: Services>(
    shell_output: crate::ShellOutput,
    truncated_output: TruncatedShellOutput,
    services: &S,
) -> anyhow::Result<String> {
    let mut metadata = FrontMatter::default().add("command", &shell_output.output.command);

    if let Some(exit_code) = shell_output.output.exit_code {
        metadata = metadata.add("exit_code", exit_code);
    }

    if truncated_output.stdout_truncated {
        metadata = metadata.add(
            "total_stdout_lines",
            shell_output.output.stdout.lines().count(),
        );
    }

    if truncated_output.stderr_truncated {
        metadata = metadata.add(
            "total_stderr_lines",
            shell_output.output.stderr.lines().count(),
        );
    }

    // Combine outputs
    let mut outputs = vec![];
    if !truncated_output.stdout.is_empty() {
        outputs.push(truncated_output.stdout.clone());
    }
    if !truncated_output.stderr.is_empty() {
        outputs.push(truncated_output.stderr.clone());
    }

    let mut result = if outputs.is_empty() {
        format!(
            "Command {} with no output.",
            if shell_output.output.success() {
                "executed successfully"
            } else {
                "failed"
            }
        )
    } else {
        outputs.join("\n")
    };

    result = format!("{metadata}{result}");

    // Create temp file if needed
    if let Some(path) = truncated_output
        .create_temp_file_if_needed(services)
        .await?
    {
        result.push_str(&format!(
            "\n<truncated>content is truncated, remaining content can be read from path:{}</truncated>",
            path.display()
        ));
    }

    if shell_output.output.success() {
        Ok(result)
    } else {
        anyhow::bail!(result)
    }
}

fn format_fs_undo(output: String) -> String {
    format!("Successfully undid last operation on path: {output}")
}

fn format_fs_patch(
    path: &str,
    warning: Option<String>,
    diff: String,
    total_chars: usize,
) -> anyhow::Result<String> {
    let metadata = FrontMatter::default()
        .add("path", path)
        .add("total_chars", total_chars)
        .add_optional("warning", warning.as_ref());

    Ok(format!("{metadata}{diff}"))
}

async fn format_fs_search_truncated<S: Services>(
    truncated_output: TruncatedSearchOutput,
    services: &S,
) -> anyhow::Result<String> {
    let metadata = FrontMatter::default()
        .add("path", &truncated_output.path)
        .add_optional("regex", truncated_output.regex.as_ref())
        .add_optional("file_pattern", truncated_output.file_pattern.as_ref())
        .add("total_lines", truncated_output.total_lines)
        .add("start_line", 1)
        .add(
            "end_line",
            truncated_output.total_lines.min(truncated_output.max_lines),
        );

    let mut result = metadata.to_string();
    result.push_str(&truncated_output.output);

    // Create temp file if needed
    if let Some(path) = truncated_output
        .create_temp_file_if_needed(services)
        .await?
    {
        result.push_str(&format!(
            "\n<truncation>content is truncated to {} lines, remaining content can be read from path:{}</truncation>",
            truncated_output.max_lines,
            path.display()
        ));
    }

    Ok(result)
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
    let is_range_relevant = out.is_explicit_range || out.is_truncated;

    let mut metadata = FrontMatter::default().add("path", &out.path);

    if is_range_relevant {
        metadata = metadata
            .add("start_line", out.start_line)
            .add("end_line", out.end_line)
            .add("total_lines", out.total_lines);
    }

    Ok(format!("{metadata}{}", out.content))
}
