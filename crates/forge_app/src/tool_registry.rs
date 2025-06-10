use std::cmp::min;
use std::path::Path;
use std::sync::Arc;

use forge_display::{DiffFormat, GrepFormat, TitleFormat};
use forge_domain::{
    AttemptCompletion, FSSearch, Tool, ToolCallContext, ToolCallFull, ToolDefinition, ToolName,
    ToolResult, Tools,
};
use regex::Regex;
use strum::IntoEnumIterator;

use crate::utils::display_path;
use crate::{
    Content, EnvironmentService, FetchOutput, FollowUpService, FsCreateOutput, FsCreateService,
    FsPatchService, FsReadService, FsRemoveService, FsSearchService, FsUndoOutput, FsUndoService,
    NetFetchService, PatchOutput, ReadOutput, SearchResult, Services, ShellOutput, ShellService,
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
        input: Tools,
        context: &mut ToolCallContext,
    ) -> anyhow::Result<crate::ToolOutput> {
        match input {
            Tools::FSRead(input) => {
                let is_explicit_range = input.start_line.is_some() | input.end_line.is_some();

                let output = self
                    .services
                    .fs_read_service()
                    .read(input.path.clone(), input.start_line, input.end_line)
                    .await?;
                let env = self.services.environment_service().get_environment();
                let display_path = display_path(&env, Path::new(&input.path))?;
                let is_truncated = output.total_lines > output.end_line;

                send_read_context(
                    context,
                    &output,
                    &display_path,
                    is_explicit_range,
                    is_truncated,
                )
                .await?;

                Ok(crate::ToolOutput::FsRead(output))
            }
            Tools::FSWrite(input) => {
                let out = self
                    .services
                    .fs_create_service()
                    .create(input.path.clone(), input.content, input.overwrite, true)
                    .await?;
                send_write_context(context, &out, &input.path, self.services.as_ref()).await?;

                Ok(crate::ToolOutput::from(out))
            }
            Tools::FSSearch(input) => {
                let output = self
                    .services
                    .fs_search_service()
                    .search(
                        input.path.clone(),
                        input.regex.clone(),
                        input.file_pattern.clone(),
                    )
                    .await?;

                send_fs_search_context(self.services.as_ref(), context, &input, &output).await?;

                Ok(crate::ToolOutput::from(output))
            }
            Tools::FSRemove(input) => {
                let output = self
                    .services
                    .fs_remove_service()
                    .remove(input.path.clone())
                    .await?;

                send_fs_remove_context(context, &input.path, self.services.as_ref()).await?;

                Ok(crate::ToolOutput::from(output))
            }
            Tools::FSPatch(input) => {
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
                send_fs_patch_context(context, &input.path, &output, self.services.as_ref())
                    .await?;

                Ok(crate::ToolOutput::from(output))
            }
            Tools::FSUndo(input) => {
                let output = self.services.fs_undo_service().undo(input.path).await?;
                send_fs_undo_context(context, &output).await?;

                Ok(crate::ToolOutput::from(output))
            }
            Tools::Shell(input) => {
                let output = self
                    .services
                    .shell_service()
                    .execute(input.command, input.cwd, input.keep_ansi)
                    .await?;
                send_shell_output_context(context, &output).await?;

                Ok(crate::ToolOutput::from(output))
            }
            Tools::NetFetch(input) => {
                let output = self
                    .services
                    .net_fetch_service()
                    .fetch(input.url.clone(), input.raw)
                    .await?;

                send_net_fetch_context(context, &output, &input.url).await?;

                Ok(crate::ToolOutput::from(output))
            }
            Tools::Followup(input) => {
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
                context.set_complete().await;

                Ok(crate::ToolOutput::from(output))
            }
            Tools::AttemptCompletion(input) => {
                send_completion_context(context, input).await?;
                Ok(crate::ToolOutput::AttemptCompletion)
            }
        }
    }
    #[allow(dead_code)]
    pub async fn call(&self, input: ToolCallFull, context: &mut ToolCallContext) -> ToolResult {
        let Ok(tool_input) = serde_json::from_value::<Tools>(input.arguments) else {
            return ToolResult::new(input.name)
                .failure(anyhow::anyhow!("Failed to parse tool input arguments"));
        };

        let result = self.call_internal(tool_input.clone(), context).await;
        match result {
            Ok(out) => {
                let Ok(truncation_path) = out.to_create_temp(self.services.as_ref()).await else {
                    return ToolResult::new(input.name)
                        .failure(anyhow::anyhow!("Failed to truncate output"));
                };

                let env = self.services.environment_service().get_environment();

                out.to_tool_result(input.name, Some(tool_input), truncation_path, &env)
            }
            Err(err) => ToolResult::new(input.name).failure(err),
        }
    }
    #[allow(dead_code)]
    pub async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        Ok(Tools::iter().map(|tool| tool.definition()).collect())
    }
    #[allow(dead_code)]
    pub async fn find(&self, _: &ToolName) -> anyhow::Result<Option<Arc<Tool>>> {
        unimplemented!()
    }
}

async fn send_completion_context(
    ctx: &mut ToolCallContext,
    input: AttemptCompletion,
) -> anyhow::Result<()> {
    ctx.send_summary(input.result).await?;
    ctx.set_complete().await;

    Ok(())
}

async fn send_fs_undo_context(ctx: &mut ToolCallContext, out: &FsUndoOutput) -> anyhow::Result<()> {
    // Display a message about the file being undone
    let message = TitleFormat::debug("Undo").sub_title(out.as_str());
    ctx.send_text(message).await
}

async fn send_net_fetch_context(
    ctx: &mut ToolCallContext,
    output: &FetchOutput,
    url: &str,
) -> anyhow::Result<()> {
    ctx.send_text(TitleFormat::debug(format!("GET {}", output.code)).sub_title(url))
        .await?;

    Ok(())
}

async fn send_shell_output_context(
    ctx: &mut ToolCallContext,
    output: &ShellOutput,
) -> anyhow::Result<()> {
    let title_format = TitleFormat::debug(format!("Execute [{}]", output.shell.as_str()))
        .sub_title(&output.output.command);
    ctx.send_text(title_format).await?;
    Ok(())
}

async fn send_fs_patch_context<S: Services>(
    ctx: &mut ToolCallContext,
    path: &String,
    output: &PatchOutput,
    services: &S,
) -> anyhow::Result<()> {
    let env = services.environment_service().get_environment();

    let display_path = display_path(&env, Path::new(&path))?;
    // Generate diff between old and new content
    let diff =
        console::strip_ansi_codes(&DiffFormat::format(&output.before, &output.after)).to_string();

    ctx.send_text(format!(
        "{}",
        TitleFormat::debug("Patch").sub_title(&display_path)
    ))
    .await?;

    // Output diff either to sender or println
    ctx.send_text(&diff).await?;

    Ok(())
}

async fn send_fs_remove_context<S: Services>(
    ctx: &mut ToolCallContext,
    path: &str,
    service: &S,
) -> anyhow::Result<()> {
    let env = service.environment_service().get_environment();
    let display_path = display_path(&env, Path::new(path))?;

    let message = TitleFormat::debug("Remove").sub_title(&display_path);

    // Send the formatted message
    ctx.send_text(message).await?;
    Ok(())
}

async fn send_fs_search_context<S: Services>(
    services: &S,
    context: &mut ToolCallContext,
    input: &FSSearch,
    output: &Option<SearchResult>,
) -> anyhow::Result<()> {
    let env = services.environment_service().get_environment();
    let formatted_dir = display_path(&env, Path::new(&input.path))?;

    let title = match (&input.regex, &input.file_pattern) {
        (Some(regex), Some(pattern)) => {
            format!("Search for '{regex}' in '{pattern}' files at {formatted_dir}")
        }
        (Some(regex), None) => format!("Search for '{regex}' at {formatted_dir}"),
        (None, Some(pattern)) => format!("Search for '{pattern}' at {formatted_dir}"),
        (None, None) => format!("Search at {formatted_dir}"),
    };

    if let Some(output) = output.as_ref() {
        context.send_text(&title).await?;
        let mut formatted_output = GrepFormat::new(output.matches.clone());
        if let Some(regex) = input.regex.as_ref().and_then(|v| Regex::new(v).ok()) {
            formatted_output = formatted_output.regex(regex);
        }
        context.send_text(formatted_output.format()).await?;
    }

    Ok(())
}

async fn send_write_context<S: Services>(
    ctx: &mut ToolCallContext,
    out: &FsCreateOutput,
    path: &str,
    services: &S,
) -> anyhow::Result<()> {
    let env = services.environment_service().get_environment();
    let formatted_path = display_path(&env, Path::new(&out.path))?;
    let new_content = services
        .fs_read_service()
        .read(path.to_string(), None, None)
        .await?;
    let exists = out.previous.is_some();

    let title = if exists { "Overwrite" } else { "Create" };

    ctx.send_text(format!(
        "{}",
        TitleFormat::debug(title).sub_title(formatted_path)
    ))
    .await?;

    if let Some(old_content) = out.previous.as_ref() {
        match new_content.content {
            Content::File(new_content) => {
                let diff = DiffFormat::format(old_content, &new_content);
                ctx.send_text(diff).await?;
            }
        }
    }
    Ok(())
}

async fn send_read_context(
    ctx: &mut ToolCallContext,
    out: &ReadOutput,
    display_path: &str,
    is_explicit_range: bool,
    is_truncated: bool,
) -> anyhow::Result<()> {
    let is_range_relevant = is_explicit_range || is_truncated;
    // Set the title based on whether this was an explicit user range request
    // or an automatic limit for large files that actually needed truncation
    let title = if is_explicit_range {
        "Read (Range)"
    } else if is_truncated {
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
    subtitle.push_str(display_path);

    // Add range info if relevant
    if is_range_relevant {
        // Add range info for explicit ranges or truncated files
        subtitle.push_str(&format!(" ({range_info})"));
    }
    let message = TitleFormat::debug(title).sub_title(subtitle);
    ctx.send_text(message).await?;
    Ok(())
}
