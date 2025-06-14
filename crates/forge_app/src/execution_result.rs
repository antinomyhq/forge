use std::cmp::min;
use std::path::{Path, PathBuf};

use console::strip_ansi_codes;
use forge_display::DiffFormat;
use forge_domain::{Environment, Tools};
use forge_template::Element;

use crate::truncation::{
    create_temp_file, truncate_fetch_content, truncate_search_output, truncate_shell_output,
};
use crate::utils::display_path;
use crate::{
    Content, EnvironmentService, FsCreateOutput, FsRemoveOutput, FsUndoOutput, HttpResponse,
    PatchOutput, ReadOutput, ResponseContext, SearchResult, Services, ShellOutput,
};

#[derive(Debug, derive_more::From)]
pub enum ExecutionResult {
    FsRead(ReadOutput),
    FsCreate(FsCreateOutput),
    FsRemove(FsRemoveOutput),
    FsSearch(Option<SearchResult>),
    FsPatch(PatchOutput),
    FsUndo(FsUndoOutput),
    NetFetch(HttpResponse),
    Shell(ShellOutput),
    FollowUp(Option<String>),
    AttemptCompletion,
}

impl ExecutionResult {
    pub fn into_tool_output(
        self,
        input: Tools,
        truncation_path: Option<PathBuf>,
        env: &Environment,
    ) -> forge_domain::ToolOutput {
        match (input, self) {
            (Tools::ForgeToolFsRead(input), ExecutionResult::FsRead(out)) => match &out.content {
                Content::File(content) => {
                    let elm = Element::new("file_content")
                        .attr("path", input.path)
                        .attr("start_line", out.start_line)
                        .attr("end_line", out.end_line)
                        .attr("total_lines", content.lines().count())
                        .cdata(content);

                    forge_domain::ToolOutput::text(elm)
                }
            },
            (Tools::ForgeToolFsCreate(input), ExecutionResult::FsCreate(output)) => {
                let mut elm = if let Some(before) = output.before {
                    let diff =
                        console::strip_ansi_codes(&DiffFormat::format(&before, &input.content))
                            .to_string();
                    Element::new("file_overwritten").append(Element::new("file_diff").cdata(diff))
                } else {
                    Element::new("file_created")
                };

                elm = elm
                    .attr("path", input.path)
                    .attr("total_lines", input.content.lines().count());

                if let Some(warning) = output.warning {
                    elm = elm.append(Element::new("warning").text(warning));
                }

                forge_domain::ToolOutput::text(elm)
            }
            (Tools::ForgeToolFsRemove(input), ExecutionResult::FsRemove(_)) => {
                let display_path = display_path(env, Path::new(&input.path));
                let elem = Element::new("file_removed")
                    .attr("path", display_path)
                    .attr("status", "completed");

                forge_domain::ToolOutput::text(elem)
            }
            (Tools::ForgeToolFsSearch(input), ExecutionResult::FsSearch(output)) => match output {
                Some(out) => {
                    let max_lines = min(
                        env.max_search_lines,
                        input.max_search_lines.unwrap_or(u64::MAX),
                    );
                    let start_index = input.start_index.unwrap_or(1);
                    let start_index = if start_index > 0 { start_index - 1 } else { 0 };
                    let truncated_output =
                        truncate_search_output(&out.matches, start_index, max_lines, env);

                    let mut elm = Element::new("search_results")
                        .attr("path", &input.path)
                        .attr("total_lines", truncated_output.total_lines)
                        .attr("start_line", truncated_output.start_line)
                        .attr("end_line", truncated_output.end_line);

                    elm = elm.attr_if_some("regex", input.regex);
                    elm = elm.attr_if_some("file_pattern", input.file_pattern);

                    elm = elm.cdata(truncated_output.output.trim());

                    forge_domain::ToolOutput::text(elm)
                }
                None => {
                    let mut elm = Element::new("search_results").attr("path", &input.path);
                    elm = elm.attr_if_some("regex", input.regex);
                    elm = elm.attr_if_some("file_pattern", input.file_pattern);

                    forge_domain::ToolOutput::text(elm)
                }
            },
            (Tools::ForgeToolFsPatch(input), ExecutionResult::FsPatch(output)) => {
                let diff =
                    console::strip_ansi_codes(&DiffFormat::format(&output.before, &output.after))
                        .to_string();

                let mut elm = Element::new("file_diff")
                    .attr("path", &input.path)
                    .attr("total_lines", output.after.lines().count())
                    .cdata(diff);

                if let Some(warning) = &output.warning {
                    elm = elm.append(Element::new("warning").text(warning));
                }

                forge_domain::ToolOutput::text(elm)
            }
            (Tools::ForgeToolFsUndo(input), ExecutionResult::FsUndo(output)) => {
                match (&output.before_undo, &output.after_undo) {
                    (None, None) => {
                        let elm = Element::new("file_undo")
                            .attr("path", input.path)
                            .attr("status", "no_changes");
                        forge_domain::ToolOutput::text(elm)
                    }
                    (None, Some(after)) => {
                        let elm = Element::new("file_undo")
                            .attr("path", input.path)
                            .attr("status", "created")
                            .attr("total_lines", after.lines().count())
                            .cdata(after);
                        forge_domain::ToolOutput::text(elm)
                    }
                    (Some(before), None) => {
                        let elm = Element::new("file_undo")
                            .attr("path", input.path)
                            .attr("status", "removed")
                            .attr("total_lines", before.lines().count())
                            .cdata(before);
                        forge_domain::ToolOutput::text(elm)
                    }
                    (Some(after), Some(before)) => {
                        let diff = DiffFormat::format(before, after);
                        let elm = Element::new("file_undo")
                            .attr("path", input.path)
                            .attr("status", "restored")
                            .cdata(strip_ansi_codes(&diff));

                        forge_domain::ToolOutput::text(elm)
                    }
                }
            }
            (Tools::ForgeToolNetFetch(input), ExecutionResult::NetFetch(output)) => {
                let content_type = match output.context {
                    ResponseContext::Parsed => "text/markdown".to_string(),
                    ResponseContext::Raw => output.content_type,
                };
                let truncated_content =
                    truncate_fetch_content(&output.content, env.fetch_truncation_limit);
                let mut elm = Element::new("http_response")
                    .attr("url", &input.url)
                    .attr("status_code", output.code)
                    .attr("start_char", 0)
                    .attr(
                        "end_char",
                        env.fetch_truncation_limit.min(output.content.len()),
                    )
                    .attr("total_chars", output.content.len())
                    .attr("content_type", content_type);

                elm = elm.append(Element::new("body").cdata(truncated_content.content));
                if let Some(path) = truncation_path.as_ref() {
                    elm = elm.append(Element::new("truncated").text(
                        format!(
                            "Content is truncated to {} chars, remaining content can be read from path: {}",
                            env.fetch_truncation_limit, path.display())
                    ));
                }

                forge_domain::ToolOutput::text(elm)
            }
            (_, ExecutionResult::Shell(output)) => {
                let mut parent_elem = Element::new("shell_output");
                let mut metadata_elem = Element::new("metadata")
                    .append(Element::new("command").cdata(&output.output.command))
                    .append(Element::new("shell").text(&output.shell));
                if let Some(exit_code) = output.output.exit_code {
                    metadata_elem = metadata_elem.append(Element::new("exit_code").text(exit_code))
                }

                parent_elem = parent_elem.append(metadata_elem);

                let truncated_output = truncate_shell_output(
                    &output.output.stdout,
                    &output.output.stderr,
                    env.stdout_max_prefix_length,
                    env.stdout_max_suffix_length,
                );
                let total_stdout = output.output.stdout.lines().count();
                let suffix_stdout = truncated_output.stdout_suffix_size;

                let mut stdout_elem = (!truncated_output.stdout.is_empty()).then_some(
                    Element::new("stdout")
                        .append(
                            Element::new("displayed_lines")
                                .text(truncated_output.stdout_prefix_count),
                        )
                        .append(Element::new("total_lines").text(total_stdout))
                        .append(Element::new("content").cdata(truncated_output.stdout)),
                );

                let total_stderr = output.output.stderr.lines().count();
                let suffix_stderr = truncated_output.stderr_suffix_size;
                let mut stderr_elem = (!truncated_output.stderr.is_empty()).then_some(
                    Element::new("stderr")
                        .append(Element::new("displayed_lines").text(suffix_stderr))
                        .append(Element::new("total_lines").text(total_stderr))
                        .append(Element::new("content").cdata(truncated_output.stderr)),
                );

                let stdout_lines = output.output.stdout.lines().count();
                let stderr_lines = output.output.stderr.lines().count();

                if let Some(path) = (truncated_output.stdout_truncated
                    || truncated_output.stderr_truncated)
                    .then(|| truncation_path.as_ref().map(|p| p.display().to_string()))
                    .flatten()
                {
                    let mut full_content_file = Element::new("full_content_file")
                        .append(Element::new("total_lines").text(stdout_lines + stderr_lines))
                        .append(Element::new("path").text(path));

                    if truncated_output.stdout_truncated {
                        full_content_file = full_content_file.append(
                            Element::new("stdout_line_range")
                                .attr("start", 2)
                                .attr("end", stdout_lines + 1),
                        );
                        stdout_elem = stdout_elem.map(|v| {
                            v.append(create_truncation_info(
                                truncated_output.stdout_prefix_count,
                                suffix_stdout,
                                truncated_output.stdout_hidden_count,
                            ))
                        });
                    }

                    if truncated_output.stderr_truncated {
                        let start = stdout_lines + 2;
                        let end = stdout_lines + stderr_lines + 2;
                        full_content_file = full_content_file.append(
                            Element::new("stderr_line_range")
                                .attr("start", start)
                                .attr("end", end),
                        );
                        stderr_elem = stderr_elem.map(|v| {
                            v.append(create_truncation_info(
                                truncated_output.stderr_prefix_count,
                                suffix_stderr,
                                truncated_output.stderr_hidden_count,
                            ))
                        });
                    }

                    parent_elem = parent_elem.append(full_content_file);
                }

                parent_elem = parent_elem.append(stdout_elem);
                parent_elem = parent_elem.append(stderr_elem);

                forge_domain::ToolOutput::text(parent_elem)
            }
            (_, ExecutionResult::FollowUp(output)) => match output {
                None => {
                    let elm = Element::new("interrupted").text("No feedback provided");
                    forge_domain::ToolOutput::text(elm)
                }
                Some(content) => {
                    let elm = Element::new("feedback").text(content);
                    forge_domain::ToolOutput::text(elm)
                }
            },
            (_, ExecutionResult::AttemptCompletion) => forge_domain::ToolOutput::text(
                Element::new("success")
                    .text("[Task was completed successfully. Now wait for user feedback]"),
            ),
            // Panic case for mismatched execution result and input tool types
            (input_tool, execution_result) => {
                panic!(
                    "Unhandled tool execution result: input_tool={input_tool:?}, execution_result={execution_result:?}"
                );
            }
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
            ExecutionResult::FsSearch(_) => Ok(None),
            ExecutionResult::FsPatch(_) => Ok(None),
            ExecutionResult::FsUndo(_) => Ok(None),
            ExecutionResult::NetFetch(output) => {
                let original_length = output.content.len();
                let is_truncated = original_length
                    > services
                        .environment_service()
                        .get_environment()
                        .fetch_truncation_limit;

                if is_truncated {
                    let path =
                        create_temp_file(services, "forge_fetch_", ".txt", &output.content).await?;

                    Ok(Some(path))
                } else {
                    Ok(None)
                }
            }
            ExecutionResult::Shell(output) => {
                let env = services.environment_service().get_environment();
                let stdout_lines = output.output.stdout.lines().count();
                let stderr_lines = output.output.stderr.lines().count();
                let stdout_truncated =
                    stdout_lines > env.stdout_max_prefix_length + env.stdout_max_suffix_length;
                let stderr_truncated =
                    stderr_lines > env.stdout_max_prefix_length + env.stdout_max_suffix_length;

                if stdout_truncated || stderr_truncated {
                    let path = create_temp_file(
                        services,
                        "forge_shell_",
                        ".md",
                        &format!(
                            "command:{}\n<stdout>{}</stdout>\n<stderr>{}</stderr>",
                            output.output.command, output.output.stdout, output.output.stderr
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

fn create_truncation_info(
    prefix_count: usize,
    suffix_count: usize,
    hidden_count: usize,
) -> Element {
    Element::new("truncation_info")
        .append(Element::new("head_lines").text(prefix_count))
        .append(Element::new("tail_lines").text(suffix_count))
        .append(Element::new("omitted_lines").text(hidden_count))
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;
    use std::path::PathBuf;

    use forge_domain::{FSRead, ToolValue, Tools};

    use super::*;
    use crate::{Match, MatchResult};

    fn fixture_environment() -> Environment {
        Environment {
            os: "linux".to_string(),
            pid: 12345,
            cwd: PathBuf::from("/home/user/project"),
            home: Some(PathBuf::from("/home/user")),
            shell: "/bin/bash".to_string(),
            base_path: PathBuf::from("/home/user/project"),
            provider: forge_domain::Provider::OpenAI {
                url: "https://api.openai.com/v1/".parse().unwrap(),
                key: Some("test-key".to_string()),
            },
            retry_config: forge_domain::RetryConfig {
                initial_backoff_ms: 1000,
                min_delay_ms: 500,
                backoff_factor: 2,
                max_retry_attempts: 3,
                retry_status_codes: vec![429, 500, 502, 503, 504],
            },
            max_search_lines: 25,
            fetch_truncation_limit: 55,
            max_read_size: 10,
            stdout_max_prefix_length: 10,
            stdout_max_suffix_length: 10,
            http: Default::default(),
        }
    }

    fn to_value(output: forge_domain::ToolOutput) -> String {
        let values = output.values;
        let mut result = String::new();
        values.into_iter().for_each(|value| match value {
            ToolValue::Text(txt) => {
                writeln!(result, "{}", txt).unwrap();
            }
            ToolValue::Image(image) => {
                writeln!(result, "Image with mime type: {}", image.mime_type()).unwrap();
            }
            ToolValue::Empty => {
                writeln!(result, "Empty value").unwrap();
            }
        });

        result
    }

    #[test]
    fn test_fs_read_basic() {
        let fixture = ExecutionResult::FsRead(ReadOutput {
            content: Content::File("Hello, world!\nThis is a test file.".to_string()),
            start_line: 1,
            end_line: 2,
            total_lines: 2,
        });

        let input = Tools::ForgeToolFsRead(FSRead {
            path: "/home/user/test.txt".to_string(),
            start_line: None,
            end_line: None,
            explanation: Some("Test explanation".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_read_basic_special_chars() {
        let fixture = ExecutionResult::FsRead(ReadOutput {
            content: Content::File("struct Foo<T>{ name: T }".to_string()),
            start_line: 1,
            end_line: 1,
            total_lines: 1,
        });

        let input = Tools::ForgeToolFsRead(FSRead {
            path: "/home/user/test.txt".to_string(),
            start_line: None,
            end_line: None,
            explanation: Some("Test explanation".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_read_with_explicit_range() {
        let fixture = ExecutionResult::FsRead(ReadOutput {
            content: Content::File("Line 1\nLine 2\nLine 3".to_string()),
            start_line: 2,
            end_line: 3,
            total_lines: 5,
        });

        let input = Tools::ForgeToolFsRead(FSRead {
            path: "/home/user/test.txt".to_string(),
            start_line: Some(2),
            end_line: Some(3),
            explanation: Some("Test explanation".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_read_with_truncation_path() {
        let fixture = ExecutionResult::FsRead(ReadOutput {
            content: Content::File("Truncated content".to_string()),
            start_line: 1,
            end_line: 100,
            total_lines: 200,
        });

        let input = Tools::ForgeToolFsRead(FSRead {
            path: "/home/user/large_file.txt".to_string(),
            start_line: None,
            end_line: None,
            explanation: Some("Test explanation".to_string()),
        });

        let env = fixture_environment();
        let truncation_path = Some(PathBuf::from("/tmp/truncated_content.txt"));

        let actual = fixture.into_tool_output(input, truncation_path, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_create_basic() {
        let fixture = ExecutionResult::FsCreate(FsCreateOutput {
            path: "/home/user/new_file.txt".to_string(),
            before: None,
            warning: None,
        });

        let input = Tools::ForgeToolFsCreate(forge_domain::FSWrite {
            path: "/home/user/new_file.txt".to_string(),
            content: "Hello, world!".to_string(),
            overwrite: false,
            explanation: Some("Creating a new file".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_create_overwrite() {
        let fixture = ExecutionResult::FsCreate(FsCreateOutput {
            path: "/home/user/existing_file.txt".to_string(),
            before: Some("Old content".to_string()),
            warning: None,
        });

        let input = Tools::ForgeToolFsCreate(forge_domain::FSWrite {
            path: "/home/user/existing_file.txt".to_string(),
            content: "New content for the file".to_string(),
            overwrite: true,
            explanation: Some("Overwriting existing file".to_string()),
        });

        let env = fixture_environment();
        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_output_no_truncation() {
        let fixture = ExecutionResult::Shell(ShellOutput {
            output: forge_domain::CommandOutput {
                command: "echo hello".to_string(),
                stdout: "hello\nworld".to_string(),
                stderr: "".to_string(),
                exit_code: Some(0),
            },
            shell: "/bin/bash".to_string(),
        });

        let input = Tools::ForgeToolProcessShell(forge_domain::Shell {
            command: "echo hello".to_string(),
            cwd: "/home/user".into(),
            explanation: Some("Test shell command".to_string()),
            keep_ansi: false,
        });

        let env = fixture_environment();
        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_output_stdout_truncation_only() {
        // Create stdout with more lines than the truncation limit
        let mut stdout_lines = Vec::new();
        for i in 1..=25 {
            stdout_lines.push(format!("stdout line {}", i));
        }
        let stdout = stdout_lines.join("\n");

        let fixture = ExecutionResult::Shell(ShellOutput {
            output: forge_domain::CommandOutput {
                command: "long_command".to_string(),
                stdout,
                stderr: "".to_string(),
                exit_code: Some(0),
            },
            shell: "/bin/bash".to_string(),
        });

        let input = Tools::ForgeToolProcessShell(forge_domain::Shell {
            command: "long_command".to_string(),
            cwd: "/home/user".into(),
            explanation: Some("Test shell command with stdout truncation".to_string()),
            keep_ansi: false,
        });

        let env = fixture_environment();
        let truncation_path = Some(PathBuf::from("/tmp/shell_output.md"));
        let actual = fixture.into_tool_output(input, truncation_path, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_output_stderr_truncation_only() {
        // Create stderr with more lines than the truncation limit
        let mut stderr_lines = Vec::new();
        for i in 1..=25 {
            stderr_lines.push(format!("stderr line {}", i));
        }
        let stderr = stderr_lines.join("\n");

        let fixture = ExecutionResult::Shell(ShellOutput {
            output: forge_domain::CommandOutput {
                command: "error_command".to_string(),
                stdout: "".to_string(),
                stderr,
                exit_code: Some(1),
            },
            shell: "/bin/bash".to_string(),
        });

        let input = Tools::ForgeToolProcessShell(forge_domain::Shell {
            command: "error_command".to_string(),
            cwd: "/home/user".into(),
            explanation: Some("Test shell command with stderr truncation".to_string()),
            keep_ansi: false,
        });

        let env = fixture_environment();
        let truncation_path = Some(PathBuf::from("/tmp/shell_output.md"));
        let actual = fixture.into_tool_output(input, truncation_path, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_output_both_stdout_stderr_truncation() {
        // Create both stdout and stderr with more lines than the truncation limit
        let mut stdout_lines = Vec::new();
        for i in 1..=25 {
            stdout_lines.push(format!("stdout line {}", i));
        }
        let stdout = stdout_lines.join("\n");

        let mut stderr_lines = Vec::new();
        for i in 1..=30 {
            stderr_lines.push(format!("stderr line {}", i));
        }
        let stderr = stderr_lines.join("\n");

        let fixture = ExecutionResult::Shell(ShellOutput {
            output: forge_domain::CommandOutput {
                command: "complex_command".to_string(),
                stdout,
                stderr,
                exit_code: Some(0),
            },
            shell: "/bin/bash".to_string(),
        });

        let input = Tools::ForgeToolProcessShell(forge_domain::Shell {
            command: "complex_command".to_string(),
            cwd: "/home/user".into(),
            explanation: Some(
                "Test shell command with both stdout and stderr truncation".to_string(),
            ),
            keep_ansi: false,
        });

        let env = fixture_environment();
        let truncation_path = Some(PathBuf::from("/tmp/shell_output.md"));
        let actual = fixture.into_tool_output(input, truncation_path, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_output_exact_boundary_stdout() {
        // Create stdout with exactly the truncation limit (prefix + suffix = 20 lines)
        let mut stdout_lines = Vec::new();
        for i in 1..=20 {
            stdout_lines.push(format!("stdout line {}", i));
        }
        let stdout = stdout_lines.join("\n");

        let fixture = ExecutionResult::Shell(ShellOutput {
            output: forge_domain::CommandOutput {
                command: "boundary_command".to_string(),
                stdout,
                stderr: "".to_string(),
                exit_code: Some(0),
            },
            shell: "/bin/bash".to_string(),
        });

        let input = Tools::ForgeToolProcessShell(forge_domain::Shell {
            command: "boundary_command".to_string(),
            cwd: "/home/user".into(),
            explanation: Some("Test shell command at exact boundary".to_string()),
            keep_ansi: false,
        });

        let env = fixture_environment();
        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_output_single_line_each() {
        let fixture = ExecutionResult::Shell(ShellOutput {
            output: forge_domain::CommandOutput {
                command: "simple_command".to_string(),
                stdout: "single stdout line".to_string(),
                stderr: "single stderr line".to_string(),
                exit_code: Some(0),
            },
            shell: "/bin/bash".to_string(),
        });

        let input = Tools::ForgeToolProcessShell(forge_domain::Shell {
            command: "simple_command".to_string(),
            cwd: "/home/user".into(),
            explanation: Some("Test shell command with single lines".to_string()),
            keep_ansi: false,
        });

        let env = fixture_environment();
        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_output_empty_streams() {
        let fixture = ExecutionResult::Shell(ShellOutput {
            output: forge_domain::CommandOutput {
                command: "silent_command".to_string(),
                stdout: "".to_string(),
                stderr: "".to_string(),
                exit_code: Some(0),
            },
            shell: "/bin/bash".to_string(),
        });

        let input = Tools::ForgeToolProcessShell(forge_domain::Shell {
            command: "silent_command".to_string(),
            cwd: "/home/user".into(),
            explanation: Some("Test shell command with empty output".to_string()),
            keep_ansi: false,
        });

        let env = fixture_environment();
        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_output_line_number_calculation() {
        // Test specific line number calculations for 1-based indexing
        let mut stdout_lines = Vec::new();
        for i in 1..=15 {
            stdout_lines.push(format!("stdout {}", i));
        }
        let stdout = stdout_lines.join("\n");

        let mut stderr_lines = Vec::new();
        for i in 1..=12 {
            stderr_lines.push(format!("stderr {}", i));
        }
        let stderr = stderr_lines.join("\n");

        let fixture = ExecutionResult::Shell(ShellOutput {
            output: forge_domain::CommandOutput {
                command: "line_test_command".to_string(),
                stdout,
                stderr,
                exit_code: Some(0),
            },
            shell: "/bin/bash".to_string(),
        });

        let input = Tools::ForgeToolProcessShell(forge_domain::Shell {
            command: "line_test_command".to_string(),
            cwd: "/home/user".into(),
            explanation: Some("Test line number calculation".to_string()),
            keep_ansi: false,
        });

        let env = fixture_environment();
        let truncation_path = Some(PathBuf::from("/tmp/shell_output.md"));
        let actual = fixture.into_tool_output(input, truncation_path, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_output() {
        // Create a large number of search matches to trigger truncation
        let mut matches = Vec::new();
        let total_lines = 50;
        for i in 1..=total_lines {
            matches.push(Match {
                path: "/home/user/project/foo.txt".to_string(),
                result: Some(MatchResult::Found {
                    line: format!("Match line {}: Test", i),
                    line_number: i,
                }),
            });
        }

        let fixture = ExecutionResult::FsSearch(Some(SearchResult { matches }));

        let input = Tools::ForgeToolFsSearch(forge_domain::FSSearch {
            path: "/home/user/project".to_string(),
            regex: Some("search".to_string()),
            start_index: Some(6),
            max_search_lines: Some(30), // This will be limited by env.max_search_lines (25)
            file_pattern: Some("*.txt".to_string()),
            explanation: Some("Testing truncated search output".to_string()),
        });

        let env = fixture_environment(); // max_search_lines is 25

        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_max_output() {
        // Create a large number of search matches to trigger truncation
        let mut matches = Vec::new();
        let total_lines = 50; // Total lines found.
        for i in 1..=total_lines {
            matches.push(Match {
                path: "/home/user/project/foo.txt".to_string(),
                result: Some(MatchResult::Found {
                    line: format!("Match line {}: Test", i),
                    line_number: i,
                }),
            });
        }

        let fixture = ExecutionResult::FsSearch(Some(SearchResult { matches }));

        let input = Tools::ForgeToolFsSearch(forge_domain::FSSearch {
            path: "/home/user/project".to_string(),
            regex: Some("search".to_string()),
            start_index: Some(6),
            max_search_lines: Some(30), // This will be limited by env.max_search_lines (25)
            file_pattern: Some("*.txt".to_string()),
            explanation: Some("Testing truncated search output".to_string()),
        });

        let mut env = fixture_environment();
        // Total lines found are 50, but we limit to 10 for this test
        env.max_search_lines = 10;

        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_no_matches() {
        let fixture = ExecutionResult::FsSearch(None);

        let input = Tools::ForgeToolFsSearch(forge_domain::FSSearch {
            path: "/home/user/empty_project".to_string(),
            regex: Some("nonexistent".to_string()),
            start_index: None,
            max_search_lines: None,
            file_pattern: None,
            explanation: Some("Testing search with no matches".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_create_with_warning() {
        let fixture = ExecutionResult::FsCreate(FsCreateOutput {
            path: "/home/user/file_with_warning.txt".to_string(),
            before: None,
            warning: Some("File created in non-standard location".to_string()),
        });

        let input = Tools::ForgeToolFsCreate(forge_domain::FSWrite {
            path: "/home/user/file_with_warning.txt".to_string(),
            content: "Content with warning".to_string(),
            overwrite: false,
            explanation: Some("Creating file with warning".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_remove_success() {
        let fixture = ExecutionResult::FsRemove(FsRemoveOutput {});

        let input = Tools::ForgeToolFsRemove(forge_domain::FSRemove {
            path: "/home/user/file_to_delete.txt".to_string(),
            explanation: Some("Removing unnecessary file".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_with_results() {
        let fixture = ExecutionResult::FsSearch(Some(SearchResult {
            matches: vec![
                Match {
                    path: "file1.txt".to_string(),
                    result: Some(MatchResult::Found {
                        line_number: 1,
                        line: "Hello world".to_string(),
                    }),
                },
                Match {
                    path: "file2.txt".to_string(),
                    result: Some(MatchResult::Found {
                        line_number: 3,
                        line: "Hello universe".to_string(),
                    }),
                },
            ],
        }));

        let input = Tools::ForgeToolFsSearch(forge_domain::FSSearch {
            path: "/home/user/project".to_string(),
            regex: Some("Hello".to_string()),
            start_index: None,
            max_search_lines: None,
            file_pattern: Some("*.txt".to_string()),
            explanation: Some("Searching for Hello pattern".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_no_results() {
        let fixture = ExecutionResult::FsSearch(None);

        let input = Tools::ForgeToolFsSearch(forge_domain::FSSearch {
            path: "/home/user/project".to_string(),
            regex: Some("NonExistentPattern".to_string()),
            start_index: None,
            max_search_lines: None,
            file_pattern: None,
            explanation: Some("Searching for non-existent pattern".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_patch_basic() {
        let fixture = ExecutionResult::FsPatch(PatchOutput {
            warning: None,
            before: "Hello world\nThis is a test".to_string(),
            after: "Hello universe\nThis is a test".to_string(),
        });

        let input = Tools::ForgeToolFsPatch(forge_domain::FSPatch {
            path: "/home/user/test.txt".to_string(),
            search: Some("world".to_string()),
            operation: forge_domain::PatchOperation::Replace,
            content: "universe".to_string(),
            explanation: Some("Replacing world with universe".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_patch_with_warning() {
        let fixture = ExecutionResult::FsPatch(PatchOutput {
            warning: Some("Large file modification".to_string()),
            before: "line1\nline2".to_string(),
            after: "line1\nnew line\nline2".to_string(),
        });

        let input = Tools::ForgeToolFsPatch(forge_domain::FSPatch {
            path: "/home/user/large_file.txt".to_string(),
            search: Some("line1".to_string()),
            operation: forge_domain::PatchOperation::Append,
            content: "\nnew line".to_string(),
            explanation: Some("Adding new line after line1".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_undo_no_changes() {
        let fixture = ExecutionResult::FsUndo(FsUndoOutput { before_undo: None, after_undo: None });

        let input = Tools::ForgeToolFsUndo(forge_domain::FSUndo {
            path: "/home/user/unchanged_file.txt".to_string(),
            explanation: Some("Attempting to undo file with no changes".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_undo_file_created() {
        let fixture = ExecutionResult::FsUndo(FsUndoOutput {
            before_undo: None,
            after_undo: Some("New file content\nLine 2\nLine 3".to_string()),
        });

        let input = Tools::ForgeToolFsUndo(forge_domain::FSUndo {
            path: "/home/user/new_file.txt".to_string(),
            explanation: Some("Undoing operation resulted in file creation".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_undo_file_removed() {
        let fixture = ExecutionResult::FsUndo(FsUndoOutput {
            before_undo: Some("Original file content\nThat was deleted\nDuring undo".to_string()),
            after_undo: None,
        });

        let input = Tools::ForgeToolFsUndo(forge_domain::FSUndo {
            path: "/home/user/deleted_file.txt".to_string(),
            explanation: Some("Undoing operation resulted in file removal".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_undo_file_restored() {
        let fixture = ExecutionResult::FsUndo(FsUndoOutput {
            before_undo: Some("Original content\nBefore changes".to_string()),
            after_undo: Some("Modified content\nAfter restoration".to_string()),
        });

        let input = Tools::ForgeToolFsUndo(forge_domain::FSUndo {
            path: "/home/user/restored_file.txt".to_string(),
            explanation: Some("Reverting changes to restore previous state".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_undo_success() {
        let fixture = ExecutionResult::FsUndo(FsUndoOutput {
            before_undo: Some("ABC".to_string()),
            after_undo: Some("PQR".to_string()),
        });

        let input = Tools::ForgeToolFsUndo(forge_domain::FSUndo {
            path: "/home/user/test.txt".to_string(),
            explanation: Some("Reverting changes to test file".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_net_fetch_success() {
        let fixture = ExecutionResult::NetFetch(HttpResponse {
            content: "# Example Website\n\nThis is some content from a website.".to_string(),
            code: 200,
            context: ResponseContext::Raw,
            content_type: "text/plain".to_string(),
        });

        let input = Tools::ForgeToolNetFetch(forge_domain::NetFetch {
            url: "https://example.com".to_string(),
            raw: Some(false),
            explanation: Some("Fetching content from example website".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_net_fetch_truncated() {
        let env = fixture_environment();
        let truncated_content = "Truncated Content".to_string();
        let long_content = format!(
            "{}{}",
            "A".repeat(env.fetch_truncation_limit),
            &truncated_content
        );
        let fixture = ExecutionResult::NetFetch(HttpResponse {
            content: long_content,
            code: 200,
            context: ResponseContext::Parsed,
            content_type: "text/html".to_string(),
        });
        let input = Tools::ForgeToolNetFetch(forge_domain::NetFetch {
            url: "https://example.com/large-page".to_string(),
            raw: Some(false),
            explanation: Some("Fetching large content that will be truncated".to_string()),
        });

        let truncation_path = Some(std::path::PathBuf::from("/tmp/forge_fetch_abc123.txt"));

        let actual = fixture.into_tool_output(input, truncation_path, &env);

        // make sure that the content is truncated
        assert!(
            !actual
                .values
                .get(0)
                .unwrap()
                .as_str()
                .unwrap()
                .ends_with(&truncated_content)
        );
        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_success() {
        let fixture = ExecutionResult::Shell(ShellOutput {
            output: forge_domain::CommandOutput {
                command: "ls -la".to_string(),
                stdout: "total 8\ndrwxr-xr-x  2 user user 4096 Jan  1 12:00 .\ndrwxr-xr-x 10 user user 4096 Jan  1 12:00 ..".to_string(),
                stderr: "".to_string(),
                exit_code: Some(0),
            },
            shell: "/bin/bash".to_string(),
        });

        let input = Tools::ForgeToolProcessShell(forge_domain::Shell {
            command: "ls -la".to_string(),
            cwd: std::path::PathBuf::from("/home/user"),
            keep_ansi: false,
            explanation: Some("Listing directory contents".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_follow_up_with_question() {
        let fixture =
            ExecutionResult::FollowUp(Some("Which file would you like to edit?".to_string()));

        let input = Tools::ForgeToolFollowup(forge_domain::Followup {
            question: "Which file would you like to edit?".to_string(),
            multiple: Some(false),
            option1: Some("file1.txt".to_string()),
            option2: Some("file2.txt".to_string()),
            option3: None,
            option4: None,
            option5: None,
            explanation: Some("Asking user for file selection".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_follow_up_no_question() {
        let fixture = ExecutionResult::FollowUp(None);

        let input = Tools::ForgeToolFollowup(forge_domain::Followup {
            question: "Do you want to continue?".to_string(),
            multiple: Some(false),
            option1: Some("Yes".to_string()),
            option2: Some("No".to_string()),
            option3: None,
            option4: None,
            option5: None,
            explanation: Some("Asking for user confirmation".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    #[should_panic(
        expected = r#"Unhandled tool execution result: input_tool=ForgeToolFsCreate(FSWrite { path: "/home/user/test.txt", content: "test", overwrite: false, explanation: Some("Test explanation") }), execution_result=FsRead(ReadOutput { content: File("test content"), start_line: 1, end_line: 1, total_lines: 1 })"#
    )]
    fn test_mismatch_error() {
        let fixture = ExecutionResult::FsRead(ReadOutput {
            content: Content::File("test content".to_string()),
            start_line: 1,
            end_line: 1,
            total_lines: 1,
        });

        // Intentionally provide wrong input type to test panic handling
        let input = Tools::ForgeToolFsCreate(forge_domain::FSWrite {
            path: "/home/user/test.txt".to_string(),
            content: "test".to_string(),
            overwrite: false,
            explanation: Some("Test explanation".to_string()),
        });

        let env = fixture_environment();

        // This should panic
        let _ = fixture.into_tool_output(input, None, &env);
    }
}
