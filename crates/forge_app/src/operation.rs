use std::cmp::min;
use std::path::{Path, PathBuf};

use console::strip_ansi_codes;
use derive_setters::Setters;
use forge_display::DiffFormat;
use forge_domain::{
    Environment, FSPatch, FSRead, FSRemove, FSSearch, FSUndo, FSWrite, NetFetch, TaskList,
    TaskListAppend, TaskListAppendMultiple, TaskListClear, TaskListList, TaskListUpdate,
};
use forge_template::Element;

use crate::truncation::{
    StreamElement, create_temp_file, truncate_fetch_content, truncate_search_output,
    truncate_shell_output,
};
use crate::utils::display_path;
use crate::{
    Content, EnvironmentService, FsCreateOutput, FsCreateService, FsUndoOutput, HttpResponse,
    PatchOutput, ReadOutput, ResponseContext, SearchResult, ShellOutput,
};

#[derive(Debug, Default, Setters)]
#[setters(into, strip_option)]
pub struct TempContentFiles {
    stdout: Option<PathBuf>,
    stderr: Option<PathBuf>,
}

#[derive(Debug, derive_more::From)]
pub enum Operation {
    FsRead {
        input: FSRead,
        output: ReadOutput,
    },
    FsCreate {
        input: FSWrite,
        output: FsCreateOutput,
    },
    FsRemove {
        input: FSRemove,
    },
    FsSearch {
        input: FSSearch,
        output: Option<SearchResult>,
    },
    FsPatch {
        input: FSPatch,
        output: PatchOutput,
    },
    FsUndo {
        input: FSUndo,
        output: FsUndoOutput,
    },
    NetFetch {
        input: NetFetch,
        output: HttpResponse,
    },
    Shell {
        output: ShellOutput,
    },
    FollowUp {
        output: Option<String>,
    },
    AttemptCompletion,
    TaskListAppend {
        _input: TaskListAppend,
        before: TaskList,
        after: TaskList,
    },
    TaskListAppendMultiple {
        _input: TaskListAppendMultiple,
        before: TaskList,
        after: TaskList,
    },
    TaskListUpdate {
        _input: TaskListUpdate,
        before: TaskList,
        after: TaskList,
    },
    TaskListList {
        _input: TaskListList,
        before: TaskList,
        after: TaskList,
    },
    TaskListClear {
        _input: TaskListClear,
        before: TaskList,
        after: TaskList,
    },
}

/// Helper function to create stdout or stderr elements with consistent
/// structure
fn create_stream_element<T: StreamElement>(
    stream: &T,
    full_output_path: Option<&Path>,
) -> Option<Element> {
    if stream.head_content().is_empty() {
        return None;
    }

    let mut elem = Element::new(stream.stream_name()).attr("total_lines", stream.total_lines());

    elem = if let Some(((tail, tail_start), tail_end)) = stream
        .tail_content()
        .zip(stream.tail_start_line())
        .zip(stream.tail_end_line())
    {
        elem.append(
            Element::new("head")
                .attr("display_lines", format!("1-{}", stream.head_end_line()))
                .cdata(stream.head_content()),
        )
        .append(
            Element::new("tail")
                .attr("display_lines", format!("{tail_start}-{tail_end}"))
                .cdata(tail),
        )
    } else {
        elem.cdata(stream.head_content())
    };

    if let Some(path) = full_output_path {
        elem = elem.attr("full_output", path.display());
    }

    Some(elem)
}
impl Operation {
    pub fn into_tool_output(
        self,
        content_files: TempContentFiles,
        env: &Environment,
    ) -> forge_domain::ToolOutput {
        match self {
            Operation::FsRead { input, output } => match &output.content {
                Content::File(content) => {
                    let elm = Element::new("file_content")
                        .attr("path", input.path)
                        .attr(
                            "display_lines",
                            format!("{}-{}", output.start_line, output.end_line),
                        )
                        .attr("total_lines", content.lines().count())
                        .cdata(content);

                    forge_domain::ToolOutput::text(elm)
                }
            },
            Operation::FsCreate { input, output } => {
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
            Operation::FsRemove { input } => {
                let display_path = display_path(env, Path::new(&input.path));
                let elem = Element::new("file_removed")
                    .attr("path", display_path)
                    .attr("status", "completed");

                forge_domain::ToolOutput::text(elem)
            }
            Operation::FsSearch { input, output } => match output {
                Some(out) => {
                    let max_lines = min(
                        env.max_search_lines,
                        input.max_search_lines.unwrap_or(i32::MAX) as u64,
                    );
                    let start_index = input.start_index.unwrap_or(1);
                    let start_index = if start_index > 0 { start_index - 1 } else { 0 };
                    let truncated_output =
                        truncate_search_output(&out.matches, start_index as u64, max_lines, env);

                    let mut elm = Element::new("search_results")
                        .attr("path", &input.path)
                        .attr("total_lines", truncated_output.total_lines)
                        .attr(
                            "display_lines",
                            format!(
                                "{}-{}",
                                truncated_output.start_line, truncated_output.end_line
                            ),
                        );

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
            Operation::FsPatch { input, output } => {
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
            Operation::FsUndo { input, output } => {
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
            Operation::NetFetch { input, output } => {
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
                if let Some(path) = content_files.stdout {
                    elm = elm.append(Element::new("truncated").text(
                        format!(
                            "Content is truncated to {} chars, remaining content can be read from path: {}",
                            env.fetch_truncation_limit, path.display())
                    ));
                }

                forge_domain::ToolOutput::text(elm)
            }
            Operation::Shell { output } => {
                let mut parent_elem = Element::new("shell_output")
                    .attr("command", &output.output.command)
                    .attr("shell", &output.shell);

                if let Some(exit_code) = output.output.exit_code {
                    parent_elem = parent_elem.attr("exit_code", exit_code);
                }

                let truncated_output = truncate_shell_output(
                    &output.output.stdout,
                    &output.output.stderr,
                    env.stdout_max_prefix_length,
                    env.stdout_max_suffix_length,
                );

                let stdout_elem = create_stream_element(
                    &truncated_output.stdout,
                    content_files.stdout.as_deref(),
                );

                let stderr_elem = create_stream_element(
                    &truncated_output.stderr,
                    content_files.stderr.as_deref(),
                );

                parent_elem = parent_elem.append(stdout_elem);
                parent_elem = parent_elem.append(stderr_elem);

                forge_domain::ToolOutput::text(parent_elem)
            }
            Operation::FollowUp { output } => match output {
                None => {
                    let elm = Element::new("interrupted").text("No feedback provided");
                    forge_domain::ToolOutput::text(elm)
                }
                Some(content) => {
                    let elm = Element::new("feedback").text(content);
                    forge_domain::ToolOutput::text(elm)
                }
            },
            Operation::AttemptCompletion => forge_domain::ToolOutput::text(
                Element::new("success")
                    .text("[Task was completed successfully. Now wait for user feedback]"),
            ),
            Operation::TaskListAppend { _input: _, before: _, after }
            | Operation::TaskListAppendMultiple { _input: _, before: _, after }
            | Operation::TaskListUpdate { _input: _, before: _, after }
            | Operation::TaskListList { _input: _, before: _, after }
            | Operation::TaskListClear { _input: _, before: _, after } => {
                let stats = forge_domain::TaskStats::from(&after);
                let elm = Element::new("task_list")
                    .attr("total_tasks", stats.total_tasks)
                    .attr("pending_tasks", stats.pending_tasks)
                    .attr("in_progress_tasks", stats.in_progress_tasks)
                    .attr("done_tasks", stats.done_tasks)
                    .append(after.tasks().iter().map(|task| {
                        //
                        Element::new("task")
                            .attr("id", task.id)
                            .attr("status", task.status.status_name())
                            .cdata(task.task.as_str())
                    }));
                forge_domain::ToolOutput::text(elm)
            }
        }
    }

    pub async fn to_create_temp<S: EnvironmentService + FsCreateService>(
        &self,
        services: &S,
    ) -> anyhow::Result<TempContentFiles> {
        match self {
            Operation::NetFetch { input: _, output } => {
                let original_length = output.content.len();
                let is_truncated =
                    original_length > services.get_environment().fetch_truncation_limit;
                let mut files = TempContentFiles::default();

                if is_truncated {
                    files = files.stdout(
                        create_temp_file(services, "forge_fetch_", ".txt", &output.content).await?,
                    );
                }

                Ok(files)
            }
            Operation::Shell { output } => {
                let env = services.get_environment();
                let stdout_lines = output.output.stdout.lines().count();
                let stderr_lines = output.output.stderr.lines().count();
                let stdout_truncated =
                    stdout_lines > env.stdout_max_prefix_length + env.stdout_max_suffix_length;
                let stderr_truncated =
                    stderr_lines > env.stdout_max_prefix_length + env.stdout_max_suffix_length;

                let mut files = TempContentFiles::default();

                if stdout_truncated {
                    files = files.stdout(
                        create_temp_file(
                            services,
                            "forge_shell_stdout_",
                            ".txt",
                            &output.output.stdout,
                        )
                        .await?,
                    );
                }
                if stderr_truncated {
                    files = files.stderr(
                        create_temp_file(
                            services,
                            "forge_shell_stderr_",
                            ".txt",
                            &output.output.stderr,
                        )
                        .await?,
                    );
                }

                Ok(files)
            }
            _ => Ok(TempContentFiles::default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;
    use std::path::PathBuf;

    use forge_domain::{FSRead, ToolValue};

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
            retry_config: forge_domain::RetryConfig {
                initial_backoff_ms: 1000,
                min_delay_ms: 500,
                backoff_factor: 2,
                max_retry_attempts: 3,
                retry_status_codes: vec![429, 500, 502, 503, 504],
                max_delay: None,
            },
            max_search_lines: 25,
            fetch_truncation_limit: 55,
            max_read_size: 10,
            stdout_max_prefix_length: 10,
            stdout_max_suffix_length: 10,
            http: Default::default(),
            max_file_size: 256 << 10, // 256 KiB
            cert: None,
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
        let fixture = Operation::FsRead {
            input: FSRead {
                path: "/home/user/test.txt".to_string(),
                start_line: None,
                end_line: None,
                explanation: Some("Test explanation".to_string()),
            },
            output: ReadOutput {
                content: Content::File("Hello, world!\nThis is a test file.".to_string()),
                start_line: 1,
                end_line: 2,
                total_lines: 2,
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_read_basic_special_chars() {
        let fixture = Operation::FsRead {
            input: FSRead {
                path: "/home/user/test.txt".to_string(),
                start_line: None,
                end_line: None,
                explanation: Some("Test explanation".to_string()),
            },
            output: ReadOutput {
                content: Content::File("struct Foo<T>{ name: T }".to_string()),
                start_line: 1,
                end_line: 1,
                total_lines: 1,
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_read_with_explicit_range() {
        let fixture = Operation::FsRead {
            input: FSRead {
                path: "/home/user/test.txt".to_string(),
                start_line: Some(2),
                end_line: Some(3),
                explanation: Some("Test explanation".to_string()),
            },
            output: ReadOutput {
                content: Content::File("Line 1\nLine 2\nLine 3".to_string()),
                start_line: 2,
                end_line: 3,
                total_lines: 5,
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_read_with_truncation_path() {
        let fixture = Operation::FsRead {
            input: FSRead {
                path: "/home/user/large_file.txt".to_string(),
                start_line: None,
                end_line: None,
                explanation: Some("Test explanation".to_string()),
            },
            output: ReadOutput {
                content: Content::File("Truncated content".to_string()),
                start_line: 1,
                end_line: 100,
                total_lines: 200,
            },
        };

        let env = fixture_environment();
        let truncation_path =
            TempContentFiles::default().stdout(PathBuf::from("/tmp/truncated_content.txt"));

        let actual = fixture.into_tool_output(truncation_path, &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_create_basic() {
        let fixture = Operation::FsCreate {
            input: forge_domain::FSWrite {
                path: "/home/user/new_file.txt".to_string(),
                content: "Hello, world!".to_string(),
                overwrite: false,
                explanation: Some("Creating a new file".to_string()),
            },
            output: FsCreateOutput {
                path: "/home/user/new_file.txt".to_string(),
                before: None,
                warning: None,
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_create_overwrite() {
        let fixture = Operation::FsCreate {
            input: forge_domain::FSWrite {
                path: "/home/user/existing_file.txt".to_string(),
                content: "New content for the file".to_string(),
                overwrite: true,
                explanation: Some("Overwriting existing file".to_string()),
            },
            output: FsCreateOutput {
                path: "/home/user/existing_file.txt".to_string(),
                before: Some("Old content".to_string()),
                warning: None,
            },
        };

        let env = fixture_environment();
        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_output_no_truncation() {
        let fixture = Operation::Shell {
            output: ShellOutput {
                output: forge_domain::CommandOutput {
                    command: "echo hello".to_string(),
                    stdout: "hello\nworld".to_string(),
                    stderr: "".to_string(),
                    exit_code: Some(0),
                },
                shell: "/bin/bash".to_string(),
            },
        };

        let env = fixture_environment();
        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

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

        let fixture = Operation::Shell {
            output: ShellOutput {
                output: forge_domain::CommandOutput {
                    command: "long_command".to_string(),
                    stdout,
                    stderr: "".to_string(),
                    exit_code: Some(0),
                },
                shell: "/bin/bash".to_string(),
            },
        };

        let env = fixture_environment();
        let truncation_path =
            TempContentFiles::default().stdout(PathBuf::from("/tmp/stdout_content.txt"));
        let actual = fixture.into_tool_output(truncation_path, &env);

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

        let fixture = Operation::Shell {
            output: ShellOutput {
                output: forge_domain::CommandOutput {
                    command: "error_command".to_string(),
                    stdout: "".to_string(),
                    stderr,
                    exit_code: Some(1),
                },
                shell: "/bin/bash".to_string(),
            },
        };

        let env = fixture_environment();
        let truncation_path =
            TempContentFiles::default().stderr(PathBuf::from("/tmp/stderr_content.txt"));
        let actual = fixture.into_tool_output(truncation_path, &env);

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

        let fixture = Operation::Shell {
            output: ShellOutput {
                output: forge_domain::CommandOutput {
                    command: "complex_command".to_string(),
                    stdout,
                    stderr,
                    exit_code: Some(0),
                },
                shell: "/bin/bash".to_string(),
            },
        };

        let env = fixture_environment();
        let truncation_path = TempContentFiles::default()
            .stdout(PathBuf::from("/tmp/stdout_content.txt"))
            .stderr(PathBuf::from("/tmp/stderr_content.txt"));
        let actual = fixture.into_tool_output(truncation_path, &env);

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

        let fixture = Operation::Shell {
            output: ShellOutput {
                output: forge_domain::CommandOutput {
                    command: "boundary_command".to_string(),
                    stdout,
                    stderr: "".to_string(),
                    exit_code: Some(0),
                },
                shell: "/bin/bash".to_string(),
            },
        };

        let env = fixture_environment();
        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_output_single_line_each() {
        let fixture = Operation::Shell {
            output: ShellOutput {
                output: forge_domain::CommandOutput {
                    command: "simple_command".to_string(),
                    stdout: "single stdout line".to_string(),
                    stderr: "single stderr line".to_string(),
                    exit_code: Some(0),
                },
                shell: "/bin/bash".to_string(),
            },
        };

        let env = fixture_environment();
        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_output_empty_streams() {
        let fixture = Operation::Shell {
            output: ShellOutput {
                output: forge_domain::CommandOutput {
                    command: "silent_command".to_string(),
                    stdout: "".to_string(),
                    stderr: "".to_string(),
                    exit_code: Some(0),
                },
                shell: "/bin/bash".to_string(),
            },
        };

        let env = fixture_environment();
        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

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

        let fixture = Operation::Shell {
            output: ShellOutput {
                output: forge_domain::CommandOutput {
                    command: "line_test_command".to_string(),
                    stdout,
                    stderr,
                    exit_code: Some(0),
                },
                shell: "/bin/bash".to_string(),
            },
        };

        let env = fixture_environment();
        let truncation_path = TempContentFiles::default()
            .stdout(PathBuf::from("/tmp/stdout_content.txt"))
            .stderr(PathBuf::from("/tmp/stderr_content.txt"));
        let actual = fixture.into_tool_output(truncation_path, &env);

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

        let fixture = Operation::FsSearch {
            input: forge_domain::FSSearch {
                path: "/home/user/project".to_string(),
                regex: Some("search".to_string()),
                start_index: Some(6),
                max_search_lines: Some(30), // This will be limited by env.max_search_lines (25)
                file_pattern: Some("*.txt".to_string()),
                explanation: Some("Testing truncated search output".to_string()),
            },
            output: Some(SearchResult { matches }),
        };

        let env = fixture_environment(); // max_search_lines is 25

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

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

        let fixture = Operation::FsSearch {
            input: forge_domain::FSSearch {
                path: "/home/user/project".to_string(),
                regex: Some("search".to_string()),
                start_index: Some(6),
                max_search_lines: Some(30), // This will be limited by env.max_search_lines (25)
                file_pattern: Some("*.txt".to_string()),
                explanation: Some("Testing truncated search output".to_string()),
            },
            output: Some(SearchResult { matches }),
        };

        let mut env = fixture_environment();
        // Total lines found are 50, but we limit to 10 for this test
        env.max_search_lines = 10;

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_no_matches() {
        let fixture = Operation::FsSearch {
            input: forge_domain::FSSearch {
                path: "/home/user/empty_project".to_string(),
                regex: Some("nonexistent".to_string()),
                start_index: None,
                max_search_lines: None,
                file_pattern: None,
                explanation: Some("Testing search with no matches".to_string()),
            },
            output: None,
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }
    #[test]
    fn test_task_list_empty() {
        let fixture = Operation::TaskListList {
            _input: forge_domain::TaskListList {
                explanation: Some("List empty tasks".to_string()),
            },
            before: TaskList::new(),
            after: TaskList::new(),
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_task_list_single_pending_task() {
        let mut task_list = TaskList::new();
        task_list.append("Write documentation");

        let fixture = Operation::TaskListList {
            _input: forge_domain::TaskListList {
                explanation: Some("List tasks with one pending".to_string()),
            },
            before: TaskList::new(),
            after: task_list,
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_task_list_mixed_status_tasks() {
        let mut task_list = TaskList::new();
        task_list.append("First task");
        let task2 = task_list.append("Second task");
        task_list.append("Third task");

        // Mark second task as done
        task_list.mark_done(task2.id);

        // Mark first task as in progress manually
        task_list.get_task_mut(0).unwrap().mark_in_progress();

        let fixture = Operation::TaskListList {
            _input: forge_domain::TaskListList {
                explanation: Some("List tasks with mixed statuses".to_string()),
            },
            before: TaskList::new(),
            after: task_list,
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_task_list_complex_scenario() {
        let mut task_list = TaskList::new();
        let _task1 = task_list.append("Review pull request #123");
        let _task2 = task_list.append("Fix bug in authentication");
        let task3 = task_list.append("Deploy to staging");
        let _task4 = task_list.append("Update API documentation");
        let _task5 = task_list.append("Refactor user service");

        // Mark one task as done
        task_list.mark_done(task3.id);

        // Mark some tasks as in progress manually
        task_list.get_task_mut(0).unwrap().mark_in_progress(); // Mark first task as in progress
        task_list.get_task_mut(4).unwrap().mark_in_progress(); // Mark last task as in progress

        let fixture = Operation::TaskListList {
            _input: forge_domain::TaskListList {
                explanation: Some("List complex task scenario".to_string()),
            },
            before: TaskList::new(),
            after: task_list,
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_task_list_append_operation() {
        let mut before_task_list = TaskList::new();
        before_task_list.append("Existing task");

        let mut after_task_list = before_task_list.clone();
        after_task_list.append("New task from append");

        let fixture = Operation::TaskListAppend {
            _input: forge_domain::TaskListAppend {
                task: "New task from append".to_string(),
                explanation: Some("Append new task".to_string()),
            },
            before: before_task_list,
            after: after_task_list,
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_task_list_mark_done_operation() {
        let mut before_task_list = TaskList::new();
        let task1 = before_task_list.append("Task to complete");
        before_task_list.append("Another task");

        let mut after_task_list = before_task_list.clone();
        after_task_list.update_status(task1.id, forge_domain::Status::Done);

        let fixture = Operation::TaskListUpdate {
            _input: forge_domain::TaskListUpdate {
                task_id: task1.id,
                status: forge_domain::Status::Done,
                explanation: Some("Mark task as done".to_string()),
            },
            before: before_task_list,
            after: after_task_list,
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_task_list_large_numbers() {
        let mut task_list = TaskList::new();
        // Create tasks with large IDs to test padding
        for i in 1..=15 {
            task_list.append(format!("Task number {}", i));
        }

        // Mark some tasks with different statuses
        task_list.mark_done(1);
        task_list.mark_done(5);
        task_list.mark_done(10);
        // Mark some tasks as in progress manually
        task_list.get_task_mut(1).unwrap().mark_in_progress(); // Mark task 2 as in progress
        task_list.get_task_mut(2).unwrap().mark_in_progress(); // Mark task 3 as in progress

        let fixture = Operation::TaskListList {
            _input: forge_domain::TaskListList {
                explanation: Some("List tasks with large numbers".to_string()),
            },
            before: TaskList::new(),
            after: task_list,
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_create_with_warning() {
        let fixture = Operation::FsCreate {
            input: forge_domain::FSWrite {
                path: "/home/user/file_with_warning.txt".to_string(),
                content: "Content with warning".to_string(),
                overwrite: false,
                explanation: Some("Creating file with warning".to_string()),
            },
            output: FsCreateOutput {
                path: "/home/user/file_with_warning.txt".to_string(),
                before: None,
                warning: Some("File created in non-standard location".to_string()),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_remove_success() {
        let fixture = Operation::FsRemove {
            input: forge_domain::FSRemove {
                path: "/home/user/file_to_delete.txt".to_string(),
                explanation: Some("Removing unnecessary file".to_string()),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_with_results() {
        let fixture = Operation::FsSearch {
            input: forge_domain::FSSearch {
                path: "/home/user/project".to_string(),
                regex: Some("Hello".to_string()),
                start_index: None,
                max_search_lines: None,
                file_pattern: Some("*.txt".to_string()),
                explanation: Some("Searching for Hello pattern".to_string()),
            },
            output: Some(SearchResult {
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
            }),
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_no_results() {
        let fixture = Operation::FsSearch {
            input: forge_domain::FSSearch {
                path: "/home/user/project".to_string(),
                regex: Some("NonExistentPattern".to_string()),
                start_index: None,
                max_search_lines: None,
                file_pattern: None,
                explanation: Some("Searching for non-existent pattern".to_string()),
            },
            output: None,
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_patch_basic() {
        let fixture = Operation::FsPatch {
            input: forge_domain::FSPatch {
                path: "/home/user/test.txt".to_string(),
                search: Some("world".to_string()),
                operation: forge_domain::PatchOperation::Replace,
                content: "universe".to_string(),
                explanation: Some("Replacing world with universe".to_string()),
            },
            output: PatchOutput {
                warning: None,
                before: "Hello world\nThis is a test".to_string(),
                after: "Hello universe\nThis is a test".to_string(),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_patch_with_warning() {
        let fixture = Operation::FsPatch {
            input: forge_domain::FSPatch {
                path: "/home/user/large_file.txt".to_string(),
                search: Some("line1".to_string()),
                operation: forge_domain::PatchOperation::Append,
                content: "\nnew line".to_string(),
                explanation: Some("Adding new line after line1".to_string()),
            },
            output: PatchOutput {
                warning: Some("Large file modification".to_string()),
                before: "line1\nline2".to_string(),
                after: "line1\nnew line\nline2".to_string(),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_undo_no_changes() {
        let fixture = Operation::FsUndo {
            input: forge_domain::FSUndo {
                path: "/home/user/unchanged_file.txt".to_string(),
                explanation: Some("Attempting to undo file with no changes".to_string()),
            },
            output: FsUndoOutput { before_undo: None, after_undo: None },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_undo_file_created() {
        let fixture = Operation::FsUndo {
            input: forge_domain::FSUndo {
                path: "/home/user/new_file.txt".to_string(),
                explanation: Some("Undoing operation resulted in file creation".to_string()),
            },
            output: FsUndoOutput {
                before_undo: None,
                after_undo: Some("New file content\nLine 2\nLine 3".to_string()),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_undo_file_removed() {
        let fixture = Operation::FsUndo {
            input: forge_domain::FSUndo {
                path: "/home/user/deleted_file.txt".to_string(),
                explanation: Some("Undoing operation resulted in file removal".to_string()),
            },
            output: FsUndoOutput {
                before_undo: Some(
                    "Original file content\nThat was deleted\nDuring undo".to_string(),
                ),
                after_undo: None,
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_undo_file_restored() {
        let fixture = Operation::FsUndo {
            input: forge_domain::FSUndo {
                path: "/home/user/restored_file.txt".to_string(),
                explanation: Some("Reverting changes to restore previous state".to_string()),
            },
            output: FsUndoOutput {
                before_undo: Some("Original content\nBefore changes".to_string()),
                after_undo: Some("Modified content\nAfter restoration".to_string()),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_undo_success() {
        let fixture = Operation::FsUndo {
            input: forge_domain::FSUndo {
                path: "/home/user/test.txt".to_string(),
                explanation: Some("Reverting changes to test file".to_string()),
            },
            output: FsUndoOutput {
                before_undo: Some("ABC".to_string()),
                after_undo: Some("PQR".to_string()),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_net_fetch_success() {
        let fixture = Operation::NetFetch {
            input: forge_domain::NetFetch {
                url: "https://example.com".to_string(),
                raw: Some(false),
                explanation: Some("Fetching content from example website".to_string()),
            },
            output: HttpResponse {
                content: "# Example Website\n\nThis is some content from a website.".to_string(),
                code: 200,
                context: ResponseContext::Raw,
                content_type: "text/plain".to_string(),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

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
        let fixture = Operation::NetFetch {
            input: forge_domain::NetFetch {
                url: "https://example.com/large-page".to_string(),
                raw: Some(false),
                explanation: Some("Fetching large content that will be truncated".to_string()),
            },
            output: HttpResponse {
                content: long_content,
                code: 200,
                context: ResponseContext::Parsed,
                content_type: "text/html".to_string(),
            },
        };

        let truncation_path =
            TempContentFiles::default().stdout(PathBuf::from("/tmp/forge_fetch_abc123.txt"));

        let actual = fixture.into_tool_output(truncation_path, &env);

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
        let fixture = Operation::Shell {
            output: ShellOutput {
                output: forge_domain::CommandOutput {
                    command: "ls -la".to_string(),
                    stdout: "total 8\ndrwxr-xr-x  2 user user 4096 Jan  1 12:00 .\ndrwxr-xr-x 10 user user 4096 Jan  1 12:00 ..".to_string(),
                    stderr: "".to_string(),
                    exit_code: Some(0),
                },
                shell: "/bin/bash".to_string(),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_attempt_completion() {
        let fixture = Operation::AttemptCompletion;

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_follow_up_with_question() {
        let fixture = Operation::FollowUp {
            output: Some("Which file would you like to edit?".to_string()),
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_follow_up_no_question() {
        let fixture = Operation::FollowUp { output: None };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(TempContentFiles::default(), &env);

        insta::assert_snapshot!(to_value(actual));
    }
}
