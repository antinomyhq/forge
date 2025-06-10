use std::path::{Path, PathBuf};

use forge_display::DiffFormat;
use forge_domain::{Environment, Tools};

use crate::front_matter::FrontMatter;
use crate::truncation::FETCH_MAX_LENGTH;
use crate::utils::display_path;
use crate::{
    Content, FetchOutput, FsCreateOutput, FsRemoveOutput, FsUndoOutput, PatchOutput, ReadOutput,
    SearchResult, Services, ShellOutput, create_temp_file, truncate_search_output,
};

#[derive(derive_more::From)]
pub enum ExecutionResult {
    FsRead(ReadOutput),
    FsCreate(FsCreateOutput),
    FsRemove(FsRemoveOutput),
    FsSearch(Option<SearchResult>),
    FsPatch(PatchOutput),
    FsUndo(FsUndoOutput),
    NetFetch(FetchOutput),
    Shell(ShellOutput),
    FollowUp(Option<String>),
    AttemptCompletion,
}

impl ExecutionResult {
    pub fn into_tool_output(
        self,
        input: Option<Tools>,
        truncation_path: Option<PathBuf>,
        env: &Environment,
    ) -> anyhow::Result<forge_domain::ToolOutput> {
        match self {
            ExecutionResult::FsRead(out) => {
                if let Some(Tools::ForgeToolFsRead(input)) = input {
                    let is_explicit_range = input.start_line.is_some() | input.end_line.is_some();
                    let is_range_relevant = is_explicit_range || truncation_path.is_some();

                    let mut metadata = FrontMatter::default().add("path", input.path);

                    if is_range_relevant {
                        metadata = metadata
                            .add("start_line", out.start_line)
                            .add("end_line", out.end_line)
                            .add("total_lines", out.total_lines);
                    }

                    match &out.content {
                        Content::File(content) => Ok(forge_domain::ToolOutput::text(format!(
                            "{metadata}{content}"
                        ))),
                    }
                } else {
                    unreachable!()
                }
            }
            ExecutionResult::FsCreate(out) => {
                if let Some(Tools::ForgeToolFsCreate(input)) = input {
                    let chars = input.content.len();
                    let operation = if out.previous.is_some() {
                        "OVERWRITE"
                    } else {
                        "CREATE"
                    };

                    let metadata = FrontMatter::default()
                        .add("path", &out.path)
                        .add("operation", operation)
                        .add("total_chars", chars)
                        .add_optional("Warning", out.warning.as_ref());

                    Ok(forge_domain::ToolOutput::text(metadata.to_string()))
                } else {
                    unreachable!()
                }
            }
            ExecutionResult::FsRemove(out) => {
                if let Some(Tools::ForgeToolFsRemove(input)) = input {
                    let display_path = display_path(env, Path::new(&input.path))?;
                    if out.completed {
                        Ok(forge_domain::ToolOutput::text(format!(
                            "Successfully removed file: {display_path}"
                        )))
                    } else {
                        Ok(forge_domain::ToolOutput::text(format!(
                            "File not found or already removed: {display_path}"
                        )))
                    }
                } else {
                    unreachable!()
                }
            }
            ExecutionResult::FsSearch(output) => {
                if let Some(Tools::ForgeToolFsSearch(input)) = input {
                    match output {
                        Some(out) => {
                            let truncated_output = truncate_search_output(
                                &out.matches,
                                &input.path,
                                input.regex.as_ref(),
                                input.file_pattern.as_ref(),
                            );
                            let metadata = FrontMatter::default()
                                .add("path", &truncated_output.path)
                                .add_optional("regex", truncated_output.regex.as_ref())
                                .add_optional(
                                    "file_pattern",
                                    truncated_output.file_pattern.as_ref(),
                                )
                                .add("total_lines", truncated_output.total_lines)
                                .add("start_line", 1)
                                .add(
                                    "end_line",
                                    truncated_output.total_lines.min(truncated_output.max_lines),
                                );

                            let mut result = metadata.to_string();
                            result.push_str(&truncated_output.output);

                            // Create temp file if needed
                            if let Some(path) = truncation_path {
                                result.push_str(&format!(
                                    "\n<truncation>content is truncated to {} lines, remaining content can be read from path:{}</truncation>",
                                    truncated_output.max_lines,
                                    path.display()
                                ));
                            }

                            Ok(forge_domain::ToolOutput::text(result))
                        }
                        None => Ok(forge_domain::ToolOutput::text(
                            "No matches found".to_string(),
                        )),
                    }
                } else {
                    unreachable!()
                }
            }
            ExecutionResult::FsPatch(output) => {
                if let Some(Tools::ForgeToolFsPatch(input)) = input {
                    let diff = console::strip_ansi_codes(&DiffFormat::format(
                        &output.before,
                        &output.after,
                    ))
                    .to_string();

                    let metadata = FrontMatter::default()
                        .add("path", &input.path)
                        .add("total_chars", output.after.len())
                        .add_optional("warning", output.warning.as_ref());

                    Ok(forge_domain::ToolOutput::text(format!("{metadata}{diff}")))
                } else {
                    unreachable!()
                }
            }
            ExecutionResult::FsUndo(output) => Ok(forge_domain::ToolOutput::text(format!(
                "Successfully undid last operation on path: {}",
                output.as_str()
            ))),
            ExecutionResult::NetFetch(output) => {
                if let Some(Tools::ForgeToolNetFetch(input)) = input {
                    let mut metadata = FrontMatter::default()
                        .add("URL", &input.url)
                        .add("total_chars", output.content.len())
                        .add("start_char", 0)
                        .add("end_char", FETCH_MAX_LENGTH.min(output.content.len()))
                        .add("context", &output.context);
                    if let Some(path) = truncation_path.as_ref() {
                        metadata = metadata.add(
                            "truncation",
                            format!(
                                "Content is truncated to {} chars; Remaining content can be read from path: {}",
                                FETCH_MAX_LENGTH,
                                path.display()
                            ),
                        );
                    }
                    let truncation_tag = match truncation_path.as_ref() {
                        Some(path) => {
                            format!(
                                "\n<truncation>content is truncated to {} chars, remaining content can be read from path: {}</truncation>",
                                FETCH_MAX_LENGTH,
                                path.to_string_lossy()
                            )
                        }
                        _ => String::new(),
                    };

                    Ok(forge_domain::ToolOutput::text(format!(
                        "{metadata}{truncation_tag}"
                    )))
                } else {
                    unreachable!()
                }
            }
            ExecutionResult::Shell(output) => {
                let mut metadata = FrontMatter::default().add("command", &output.output.command);
                if let Some(exit_code) = output.output.exit_code {
                    metadata = metadata.add("exit_code", exit_code);
                }

                let stdout_lines = output.output.stdout.lines().count();
                let stderr_lines = output.output.stderr.lines().count();
                let stdout_truncated = stdout_lines
                    > crate::truncation::PREFIX_LINES + crate::truncation::SUFFIX_LINES;
                let stderr_truncated = stderr_lines
                    > crate::truncation::PREFIX_LINES + crate::truncation::SUFFIX_LINES;

                if stdout_truncated {
                    metadata = metadata.add("total_stdout_lines", stdout_lines);
                }

                if stderr_truncated {
                    metadata = metadata.add("total_stderr_lines", stderr_lines);
                }

                let is_success = output.output.success();

                // Combine outputs
                let mut outputs = vec![];
                if !output.output.stdout.is_empty() {
                    outputs.push(output.output.stdout);
                }
                if !output.output.stderr.is_empty() {
                    outputs.push(output.output.stderr);
                }

                let mut result = if outputs.is_empty() {
                    format!(
                        "Command {} with no output.",
                        if is_success {
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
                if let Some(path) = truncation_path.as_ref() {
                    result.push_str(&format!(
                        "\n<truncated>content is truncated, remaining content can be read from path:{}</truncated>",
                        path.display()
                    ));
                }

                if is_success {
                    Ok(forge_domain::ToolOutput::text(result))
                } else {
                    anyhow::bail!(result)
                }
            }
            ExecutionResult::FollowUp(output) => match output {
                None => Ok(forge_domain::ToolOutput::text(
                    "User interrupted the selection".to_string(),
                )),
                Some(o) => Ok(forge_domain::ToolOutput::text(o.to_string())),
            },
            ExecutionResult::AttemptCompletion => Ok(forge_domain::ToolOutput::text(
                "[Task was completed successfully. Now wait for user feedback]".to_string(),
            )),
        }
    }

    pub async fn to_create_temp<S: Services>(
        &self,
        services: &S,
    ) -> anyhow::Result<Option<PathBuf>> {
        match self {
            ExecutionResult::FsRead(_) => Ok(None),
            ExecutionResult::FsCreate(_) => Ok(None),
            ExecutionResult::FsRemove(_) => Ok(None),
            ExecutionResult::FsSearch(search_result) => {
                if let Some(search_result) = search_result {
                    let output = search_result.matches.join("\n");
                    let is_truncated =
                        output.lines().count() as u64 > crate::truncation::SEARCH_MAX_LINES;

                    if is_truncated {
                        let path = crate::truncation::create_temp_file(
                            services,
                            "forge_find_",
                            ".md",
                            &output,
                        )
                        .await?;

                        Ok(Some(path))
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }
            ExecutionResult::FsPatch(_) => Ok(None),
            ExecutionResult::FsUndo(_) => Ok(None),
            ExecutionResult::NetFetch(out) => {
                let original_length = out.content.len();
                let is_truncated = original_length > crate::truncation::FETCH_MAX_LENGTH;

                if is_truncated {
                    let path =
                        create_temp_file(services, "forge_fetch_", ".txt", &out.content).await?;

                    Ok(Some(path))
                } else {
                    Ok(None)
                }
            }
            ExecutionResult::Shell(out) => {
                let stdout_lines = out.output.stdout.lines().count();
                let stderr_lines = out.output.stderr.lines().count();
                let stdout_truncated = stdout_lines
                    > crate::truncation::PREFIX_LINES + crate::truncation::SUFFIX_LINES;
                let stderr_truncated = stderr_lines
                    > crate::truncation::PREFIX_LINES + crate::truncation::SUFFIX_LINES;

                if stdout_truncated || stderr_truncated {
                    let path = create_temp_file(
                        services,
                        "forge_shell_",
                        ".md",
                        &format!(
                            "command:{}\n<stdout>{}</stdout>\n<stderr>{}</stderr>",
                            out.output.command, out.output.stdout, out.output.stderr
                        ),
                    )
                    .await?;

                    Ok(Some(path))
                } else {
                    Ok(None)
                }
            }
            ExecutionResult::FollowUp(_) => Ok(None),
            ExecutionResult::AttemptCompletion => Ok(None),
        }
    }
}
