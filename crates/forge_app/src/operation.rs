use std::cmp::min;
use std::path::{Path, PathBuf};

use console::strip_ansi_codes;
use derive_setters::Setters;
use forge_display::DiffFormat;
use forge_domain::{
    CodebaseSearchResults, Environment, FSPatch, FSRead, FSRemove, FSSearch, FSUndo, FSWrite,
    FileOperation, Metrics, NetFetch, PlanCreate, ToolKind,
};
use forge_template::Element;

use crate::truncation::{
    Stderr, Stdout, TruncationMode, truncate_fetch_content, truncate_search_output,
    truncate_shell_output,
};
use crate::utils::{compute_hash, format_display_path};
use crate::{
    FsCreateOutput, FsRemoveOutput, FsUndoOutput, HttpResponse, PatchOutput, PlanCreateOutput,
    ReadOutput, ResponseContext, SearchResult, ShellOutput,
};

#[derive(Debug, Default, Setters)]
#[setters(into, strip_option)]
pub struct TempContentFiles {
    stdout: Option<PathBuf>,
    stderr: Option<PathBuf>,
}

#[derive(Debug, derive_more::From)]
pub enum ToolOperation {
    FsRead {
        input: FSRead,
        output: ReadOutput,
    },
    ImageRead {
        output: forge_domain::Image,
    },
    FsCreate {
        input: FSWrite,
        output: FsCreateOutput,
    },
    FsRemove {
        input: FSRemove,
        output: FsRemoveOutput,
    },
    FsSearch {
        input: FSSearch,
        output: Option<SearchResult>,
    },
    CodebaseSearch {
        output: CodebaseSearchResults,
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
    PlanCreate {
        input: PlanCreate,
        output: PlanCreateOutput,
    },
    Skill {
        #[allow(dead_code)]
        input: forge_domain::SkillFetch,
        output: forge_domain::Skill,
    },
}

/// Trait for stream elements that can be converted to XML elements
pub trait StreamElement {
    fn stream_name(&self) -> &'static str;
    fn head_content(&self) -> &str;
    fn tail_content(&self) -> Option<&str>;
    fn total_lines(&self) -> usize;
    fn head_end_line(&self) -> usize;
    fn tail_start_line(&self) -> Option<usize>;
    fn tail_end_line(&self) -> Option<usize>;
}

impl StreamElement for Stdout {
    fn stream_name(&self) -> &'static str {
        "stdout"
    }

    fn head_content(&self) -> &str {
        &self.head
    }

    fn tail_content(&self) -> Option<&str> {
        self.tail.as_deref()
    }

    fn total_lines(&self) -> usize {
        self.total_lines
    }

    fn head_end_line(&self) -> usize {
        self.head_end_line
    }

    fn tail_start_line(&self) -> Option<usize> {
        self.tail_start_line
    }

    fn tail_end_line(&self) -> Option<usize> {
        self.tail_end_line
    }
}

impl StreamElement for Stderr {
    fn stream_name(&self) -> &'static str {
        "stderr"
    }

    fn head_content(&self) -> &str {
        &self.head
    }

    fn tail_content(&self) -> Option<&str> {
        self.tail.as_deref()
    }

    fn total_lines(&self) -> usize {
        self.total_lines
    }

    fn head_end_line(&self) -> usize {
        self.head_end_line
    }

    fn tail_start_line(&self) -> Option<usize> {
        self.tail_start_line
    }

    fn tail_end_line(&self) -> Option<usize> {
        self.tail_end_line
    }
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
impl ToolOperation {
    pub fn into_tool_output(
        self,
        tool_kind: ToolKind,
        content_files: TempContentFiles,
        env: &Environment,
        metrics: &mut Metrics,
    ) -> forge_domain::ToolOutput {
        let tool_name = tool_kind.name();
        match self {
            ToolOperation::FsRead { input, output } => {
                let content = output.content.file_content();
                let elm = Element::new("file_content")
                    .attr("path", &input.path)
                    .attr(
                        "display_lines",
                        format!("{}-{}", output.start_line, output.end_line),
                    )
                    .attr("total_lines", content.lines().count())
                    .cdata(content);

                // Track read operations
                tracing::info!(path = %input.path, tool = %tool_name, "File read");
                *metrics = metrics.clone().insert(
                    input.path.clone(),
                    FileOperation::new(tool_kind).content_hash(Some(output.content_hash.clone())),
                );

                forge_domain::ToolOutput::text(elm)
            }
            ToolOperation::ImageRead { output } => forge_domain::ToolOutput::image(output),
            ToolOperation::FsCreate { input, output } => {
                let diff_result = DiffFormat::format(
                    output.before.as_ref().unwrap_or(&"".to_string()),
                    &input.content,
                );
                let diff = console::strip_ansi_codes(diff_result.diff()).to_string();

                *metrics = metrics.clone().insert(
                    input.path.clone(),
                    FileOperation::new(tool_kind)
                        .lines_added(diff_result.lines_added())
                        .lines_removed(diff_result.lines_removed())
                        .content_hash(Some(output.content_hash.clone())),
                );

                let mut elm = if output.before.as_ref().is_some() {
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
            ToolOperation::FsRemove { input, output } => {
                // None since file was removed
                let content_hash = None;

                *metrics = metrics.clone().insert(
                    input.path.clone(),
                    FileOperation::new(tool_kind)
                        .lines_removed(output.content.lines().count() as u64)
                        .content_hash(content_hash),
                );

                let display_path = format_display_path(Path::new(&input.path), env.cwd.as_path());
                let elem = Element::new("file_removed")
                    .attr("path", display_path)
                    .attr("status", "completed");
                forge_domain::ToolOutput::text(elem)
            }

            ToolOperation::FsSearch { input, output } => match output {
                Some(out) => {
                    let max_lines = min(
                        env.max_search_lines,
                        input.max_search_lines.unwrap_or(i32::MAX) as usize,
                    );
                    let start_index = input.start_index.unwrap_or(1);
                    let start_index = if start_index > 0 { start_index - 1 } else { 0 };
                    let search_dir = Path::new(&input.path);
                    let truncated_output = truncate_search_output(
                        &out.matches,
                        start_index as usize,
                        max_lines,
                        env.max_search_result_bytes,
                        search_dir,
                    );

                    let display_lines = if truncated_output.start < truncated_output.end {
                        // 1 Line based indexing
                        let new_start = truncated_output.start.saturating_add(1);
                        format!("{}-{}", new_start, truncated_output.end)
                    } else {
                        format!("{}-{}", truncated_output.start, truncated_output.end)
                    };

                    let mut elm = Element::new("search_results")
                        .attr("path", &input.path)
                        .attr("max_bytes_allowed", env.max_search_result_bytes)
                        .attr("total_lines", truncated_output.total)
                        .attr("display_lines", display_lines);

                    elm = elm.attr_if_some("regex", input.regex);
                    elm = elm.attr_if_some("file_pattern", input.file_pattern);

                    match truncated_output.strategy {
                        TruncationMode::Byte => {
                            let reason = format!(
                                "Results truncated due to exceeding the {} bytes size limit. Please use a more specific search pattern",
                                env.max_search_result_bytes
                            );
                            elm = elm.attr("reason", reason);
                        }
                        TruncationMode::Line => {
                            let reason = format!(
                                "Results truncated due to exceeding the {max_lines} lines limit. Please use a more specific search pattern"
                            );
                            elm = elm.attr("reason", reason);
                        }
                        TruncationMode::Full => {}
                    };
                    elm = elm.cdata(truncated_output.data.join("\n"));

                    forge_domain::ToolOutput::text(elm)
                }
                None => {
                    let mut elm = Element::new("search_results").attr("path", &input.path);
                    elm = elm.attr_if_some("regex", input.regex);
                    elm = elm.attr_if_some("file_pattern", input.file_pattern);

                    forge_domain::ToolOutput::text(elm)
                }
            },
            ToolOperation::CodebaseSearch { output } => {
                let total_results: usize = output.queries.iter().map(|q| q.results.len()).sum();
                let mut root = Element::new("sem_search_results");

                if output.queries.is_empty() || total_results == 0 {
                    root = root.text("No results found for query. Try refining your search with more specific terms or different keywords.")
                } else {
                    for query_result in &output.queries {
                        root = root.append(query_result.to_element());
                    }
                }

                forge_domain::ToolOutput::text(root)
            }
            ToolOperation::FsPatch { input, output } => {
                let diff_result = DiffFormat::format(&output.before, &output.after);
                let diff = console::strip_ansi_codes(diff_result.diff()).to_string();

                let mut elm = Element::new("file_diff")
                    .attr("path", &input.path)
                    .attr("total_lines", output.after.lines().count())
                    .cdata(diff);

                if let Some(warning) = &output.warning {
                    elm = elm.append(Element::new("warning").text(warning));
                }

                *metrics = metrics.clone().insert(
                    input.path.clone(),
                    FileOperation::new(tool_kind)
                        .lines_added(diff_result.lines_added())
                        .lines_removed(diff_result.lines_removed())
                        .content_hash(Some(output.content_hash.clone())),
                );

                forge_domain::ToolOutput::text(elm)
            }
            ToolOperation::FsUndo { input, output } => {
                // Diff between snapshot state (after_undo) and modified state
                // (before_undo)
                let diff = DiffFormat::format(
                    output.after_undo.as_deref().unwrap_or(""),
                    output.before_undo.as_deref().unwrap_or(""),
                );
                let content_hash = output.after_undo.as_ref().map(|s| compute_hash(s));

                *metrics = metrics.clone().insert(
                    input.path.clone(),
                    FileOperation::new(tool_kind)
                        .lines_added(diff.lines_added())
                        .lines_removed(diff.lines_removed())
                        .content_hash(content_hash),
                );

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
                    (Some(before), Some(after)) => {
                        // This diff is between modified state (before_undo) and snapshot
                        // state (after_undo)
                        let diff = DiffFormat::format(before, after);

                        let elm = Element::new("file_undo")
                            .attr("path", input.path)
                            .attr("status", "restored")
                            .cdata(strip_ansi_codes(diff.diff()));

                        forge_domain::ToolOutput::text(elm)
                    }
                }
            }
            ToolOperation::NetFetch { input, output } => {
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
            ToolOperation::Shell { output } => {
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
                    env.stdout_max_line_length,
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
            ToolOperation::FollowUp { output } => match output {
                None => {
                    let elm = Element::new("interrupted").text("No feedback provided");
                    forge_domain::ToolOutput::text(elm)
                }
                Some(content) => {
                    let elm = Element::new("feedback").text(content);
                    forge_domain::ToolOutput::text(elm)
                }
            },
            ToolOperation::PlanCreate { input, output } => {
                let elm = Element::new("plan_created")
                    .attr("path", output.path.display().to_string())
                    .attr("plan_name", input.plan_name)
                    .attr("version", input.version);

                forge_domain::ToolOutput::text(elm)
            }
            ToolOperation::Skill { input: _, output } => {
                let mut elm = Element::new("skill_details");

                elm = elm.append({
                    let mut elm = Element::new("command");
                    if let Some(path) = output.path {
                        elm = elm.attr("location", path.display().to_string());
                    }

                    elm.cdata(output.command)
                });

                // Insert Resources
                if !output.resources.is_empty() {
                    elm = elm.append(output.resources.iter().map(|resource| {
                        Element::new("resource").text(resource.display().to_string())
                    }));
                }

                forge_domain::ToolOutput::text(elm)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;
    use std::path::PathBuf;

    use forge_domain::{FSRead, ToolValue};

    use super::*;
    use crate::{Content, Match, MatchResult};

    fn fixture_environment() -> Environment {
        use fake::{Fake, Faker};
        let max_bytes: f64 = 250.0 * 1024.0; // 250 KB
        let fixture: Environment = Faker.fake();
        fixture
            .max_search_lines(25)
            .max_search_result_bytes(max_bytes.ceil() as usize)
            .fetch_truncation_limit(55)
            .max_read_size(10)
            .stdout_max_prefix_length(10)
            .stdout_max_suffix_length(10)
            .max_file_size(256 << 10) // 256 KiB
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
        let content = "Hello, world!\nThis is a test file.";
        let hash = crate::compute_hash(content);
        let fixture = ToolOperation::FsRead {
            input: FSRead {
                path: "/home/user/test.txt".to_string(),
                start_line: None,
                end_line: None,
                show_line_numbers: true,
            },
            output: ReadOutput {
                content: Content::file(content),
                start_line: 1,
                end_line: 2,
                total_lines: 2,
                content_hash: hash,
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Read,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_read_basic_special_chars() {
        let content = "struct Foo<T>{ name: T }";
        let hash = crate::compute_hash(content);
        let fixture = ToolOperation::FsRead {
            input: FSRead {
                path: "/home/user/test.txt".to_string(),
                start_line: None,
                end_line: None,
                show_line_numbers: true,
            },
            output: ReadOutput {
                content: Content::file(content),
                start_line: 1,
                end_line: 1,
                total_lines: 1,
                content_hash: hash,
            },
        };

        let env = fixture_environment();
        let actual = fixture.into_tool_output(
            ToolKind::Read,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_read_with_explicit_range() {
        let content = "Line 1\nLine 2\nLine 3";
        let hash = crate::compute_hash(content);
        let fixture = ToolOperation::FsRead {
            input: FSRead {
                path: "/home/user/test.txt".to_string(),
                start_line: Some(2),
                end_line: Some(3),
                show_line_numbers: true,
            },
            output: ReadOutput {
                content: Content::file(content),
                start_line: 2,
                end_line: 3,
                total_lines: 5,
                content_hash: hash,
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Read,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_read_with_truncation_path() {
        let content = "Truncated content";
        let hash = crate::compute_hash(content);
        let fixture = ToolOperation::FsRead {
            input: FSRead {
                path: "/home/user/large_file.txt".to_string(),
                start_line: None,
                end_line: None,
                show_line_numbers: true,
            },
            output: ReadOutput {
                content: Content::file(content),
                start_line: 1,
                end_line: 100,
                total_lines: 200,
                content_hash: hash,
            },
        };

        let env = fixture_environment();
        let truncation_path =
            TempContentFiles::default().stdout(PathBuf::from("/tmp/truncated_content.txt"));

        let actual = fixture.into_tool_output(
            ToolKind::Read,
            truncation_path,
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_create_basic() {
        let content = "Hello, world!";
        let fixture = ToolOperation::FsCreate {
            input: forge_domain::FSWrite {
                path: "/home/user/new_file.txt".to_string(),
                content: content.to_string(),
                overwrite: false,
            },
            output: FsCreateOutput {
                path: "/home/user/new_file.txt".to_string(),
                before: None,
                warning: None,
                content_hash: compute_hash(content),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Write,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_create_overwrite() {
        let content = "New content for the file";
        let fixture = ToolOperation::FsCreate {
            input: forge_domain::FSWrite {
                path: "/home/user/existing_file.txt".to_string(),
                content: content.to_string(),
                overwrite: true,
            },
            output: FsCreateOutput {
                path: "/home/user/existing_file.txt".to_string(),
                before: Some("Old content".to_string()),
                warning: None,
                content_hash: compute_hash(content),
            },
        };

        let env = fixture_environment();
        let actual = fixture.into_tool_output(
            ToolKind::Write,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_output_no_truncation() {
        let fixture = ToolOperation::Shell {
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
        let actual = fixture.into_tool_output(
            ToolKind::Write,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

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

        let fixture = ToolOperation::Shell {
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
        let actual = fixture.into_tool_output(
            ToolKind::Shell,
            truncation_path,
            &env,
            &mut Metrics::default(),
        );

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

        let fixture = ToolOperation::Shell {
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
        let actual = fixture.into_tool_output(
            ToolKind::Shell,
            truncation_path,
            &env,
            &mut Metrics::default(),
        );

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

        let fixture = ToolOperation::Shell {
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
        let actual = fixture.into_tool_output(
            ToolKind::Shell,
            truncation_path,
            &env,
            &mut Metrics::default(),
        );

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

        let fixture = ToolOperation::Shell {
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
        let actual = fixture.into_tool_output(
            ToolKind::Shell,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_output_single_line_each() {
        let fixture = ToolOperation::Shell {
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
        let actual = fixture.into_tool_output(
            ToolKind::Shell,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_output_empty_streams() {
        let fixture = ToolOperation::Shell {
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
        let actual = fixture.into_tool_output(
            ToolKind::Shell,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

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

        let fixture = ToolOperation::Shell {
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
        let actual = fixture.into_tool_output(
            ToolKind::Shell,
            truncation_path,
            &env,
            &mut Metrics::default(),
        );

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

        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: "/home/user/project".to_string(),
                regex: Some("search".to_string()),
                start_index: Some(6),
                max_search_lines: Some(30), // This will be limited by env.max_search_lines (25)
                file_pattern: Some("*.txt".to_string()),
            },
            output: Some(SearchResult { matches }),
        };

        let env = fixture_environment(); // max_search_lines is 25

        let actual = fixture.into_tool_output(
            ToolKind::Search,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

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

        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: "/home/user/project".to_string(),
                regex: Some("search".to_string()),
                start_index: Some(6),
                max_search_lines: Some(30), // This will be limited by env.max_search_lines (25)
                file_pattern: Some("*.txt".to_string()),
            },
            output: Some(SearchResult { matches }),
        };

        let mut env = fixture_environment();
        // Total lines found are 50, but we limit to 10 for this test
        env.max_search_lines = 10;

        let actual = fixture.into_tool_output(
            ToolKind::Search,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_min_lines_but_max_line_length() {
        // Create a large number of search matches to trigger truncation
        let mut matches = Vec::new();
        let total_lines = 50; // Total lines found.
        for i in 1..=total_lines {
            matches.push(Match {
                path: "/home/user/project/foo.txt".to_string(),
                result: Some(MatchResult::Found {
                    line: format!("Match line {}: {}", i, "AB".repeat(50)),
                    line_number: i,
                }),
            });
        }

        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: "/home/user/project".to_string(),
                regex: Some("search".to_string()),
                start_index: Some(6),
                max_search_lines: Some(30), // This will be limited by env.max_search_lines (20)
                file_pattern: Some("*.txt".to_string()),
            },
            output: Some(SearchResult { matches }),
        };

        let mut env = fixture_environment();
        // Total lines found are 50, but we limit to 20 for this test
        env.max_search_lines = 20;
        let max_bytes: f64 = 0.001 * 1024.0 * 1024.0;
        env.max_search_result_bytes = max_bytes.ceil() as usize; // limit to 0.001 MB

        let actual = fixture.into_tool_output(
            ToolKind::Search,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_very_lengthy_one_line_match() {
        let mut matches = Vec::new();
        let total_lines = 1; // Total lines found.
        for i in 1..=total_lines {
            matches.push(Match {
                path: "/home/user/project/foo.txt".to_string(),
                result: Some(MatchResult::Found {
                    line: format!(
                        "Match line {}: {}",
                        i,
                        "abcdefghijklmnopqrstuvwxyz".repeat(40)
                    ),
                    line_number: i,
                }),
            });
        }

        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: "/home/user/project".to_string(),
                regex: Some("search".to_string()),
                start_index: Some(6),
                max_search_lines: Some(30), // This will be limited by env.max_search_lines (20)
                file_pattern: Some("*.txt".to_string()),
            },
            output: Some(SearchResult { matches }),
        };

        let mut env = fixture_environment();
        // Total lines found are 50, but we limit to 20 for this test
        env.max_search_lines = 20;
        let max_bytes: f64 = 0.001 * 1024.0 * 1024.0;
        env.max_search_result_bytes = max_bytes.ceil() as usize; // limit to 0.001 MB

        let actual = fixture.into_tool_output(
            ToolKind::Search,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_no_matches() {
        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: "/home/user/empty_project".to_string(),
                regex: Some("nonexistent".to_string()),
                start_index: None,
                max_search_lines: None,
                file_pattern: None,
            },
            output: None,
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Search,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_create_with_warning() {
        let content = "Content with warning";
        let fixture = ToolOperation::FsCreate {
            input: forge_domain::FSWrite {
                path: "/home/user/file_with_warning.txt".to_string(),
                content: content.to_string(),
                overwrite: false,
            },
            output: FsCreateOutput {
                path: "/home/user/file_with_warning.txt".to_string(),
                before: None,
                warning: Some("File created in non-standard location".to_string()),
                content_hash: compute_hash(content),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Write,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_remove_success() {
        let fixture = ToolOperation::FsRemove {
            input: forge_domain::FSRemove { path: "/home/user/file_to_delete.txt".to_string() },
            output: FsRemoveOutput { content: "content".to_string() },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Remove,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_with_results() {
        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: "/home/user/project".to_string(),
                regex: Some("Hello".to_string()),
                start_index: None,
                max_search_lines: None,
                file_pattern: Some("*.txt".to_string()),
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

        let actual = fixture.into_tool_output(
            ToolKind::Search,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_no_results() {
        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: "/home/user/project".to_string(),
                regex: Some("NonExistentPattern".to_string()),
                start_index: None,
                max_search_lines: None,
                file_pattern: None,
            },
            output: None,
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Search,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_patch_basic() {
        let after_content = "Hello universe\nThis is a test";
        let fixture = ToolOperation::FsPatch {
            input: forge_domain::FSPatch {
                path: "/home/user/test.txt".to_string(),
                search: Some("world".to_string()),
                operation: forge_domain::PatchOperation::Replace,
                content: "universe".to_string(),
            },
            output: PatchOutput {
                warning: None,
                before: "Hello world\nThis is a test".to_string(),
                after: after_content.to_string(),
                content_hash: compute_hash(after_content),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Patch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_patch_with_warning() {
        let after_content = "line1\nnew line\nline2";
        let fixture = ToolOperation::FsPatch {
            input: forge_domain::FSPatch {
                path: "/home/user/large_file.txt".to_string(),
                search: Some("line1".to_string()),
                operation: forge_domain::PatchOperation::Append,
                content: "\nnew line".to_string(),
            },
            output: PatchOutput {
                warning: Some("Large file modification".to_string()),
                before: "line1\nline2".to_string(),
                after: after_content.to_string(),
                content_hash: compute_hash(after_content),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Patch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_undo_no_changes() {
        let fixture = ToolOperation::FsUndo {
            input: forge_domain::FSUndo { path: "/home/user/unchanged_file.txt".to_string() },
            output: FsUndoOutput { before_undo: None, after_undo: None },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Undo,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_undo_file_created() {
        let fixture = ToolOperation::FsUndo {
            input: forge_domain::FSUndo { path: "/home/user/new_file.txt".to_string() },
            output: FsUndoOutput {
                before_undo: None,
                after_undo: Some("New file content\nLine 2\nLine 3".to_string()),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Undo,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_undo_file_removed() {
        let fixture = ToolOperation::FsUndo {
            input: forge_domain::FSUndo { path: "/home/user/deleted_file.txt".to_string() },
            output: FsUndoOutput {
                before_undo: Some(
                    "Original file content\nThat was deleted\nDuring undo".to_string(),
                ),
                after_undo: None,
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Undo,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_undo_file_restored() {
        let fixture = ToolOperation::FsUndo {
            input: forge_domain::FSUndo { path: "/home/user/restored_file.txt".to_string() },
            output: FsUndoOutput {
                before_undo: Some("Original content\nBefore changes".to_string()),
                after_undo: Some("Modified content\nAfter restoration".to_string()),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Undo,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_undo_success() {
        let fixture = ToolOperation::FsUndo {
            input: forge_domain::FSUndo { path: "/home/user/test.txt".to_string() },
            output: FsUndoOutput {
                before_undo: Some("ABC".to_string()),
                after_undo: Some("PQR".to_string()),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Undo,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_net_fetch_success() {
        let fixture = ToolOperation::NetFetch {
            input: forge_domain::NetFetch {
                url: "https://example.com".to_string(),
                raw: Some(false),
            },
            output: HttpResponse {
                content: "# Example Website\n\nThis is some content from a website.".to_string(),
                code: 200,
                context: ResponseContext::Raw,
                content_type: "text/plain".to_string(),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Fetch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

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
        let fixture = ToolOperation::NetFetch {
            input: forge_domain::NetFetch {
                url: "https://example.com/large-page".to_string(),
                raw: Some(false),
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

        let actual = fixture.into_tool_output(
            ToolKind::Fetch,
            truncation_path,
            &env,
            &mut Metrics::default(),
        );

        // make sure that the content is truncated
        assert!(
            !actual
                .values
                .first()
                .unwrap()
                .as_str()
                .unwrap()
                .ends_with(&truncated_content)
        );
        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_success() {
        let fixture = ToolOperation::Shell {
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

        let actual = fixture.into_tool_output(
            ToolKind::Shell,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_follow_up_with_question() {
        let fixture = ToolOperation::FollowUp {
            output: Some("Which file would you like to edit?".to_string()),
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Followup,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_sem_search_with_results() {
        use forge_domain::{CodebaseQueryResult, CodebaseSearchResults, Node, NodeData};

        let fixture = ToolOperation::CodebaseSearch {
            output: CodebaseSearchResults {
                queries: vec![CodebaseQueryResult {
                    query: "retry mechanism with exponential backoff".to_string(),
                    use_case: "where is the retrying logic written".to_string(),
                    results: vec![
                        Node {
                            node_id: "node1".into(),
                            node: NodeData::FileChunk(forge_domain::FileChunk {
                                file_path: "src/retry.rs".to_string(),
                                content: "fn retry_with_backoff(max_attempts: u32) {\n    let mut delay = 100;\n    for attempt in 0..max_attempts {\n        if try_operation().is_ok() {\n            return;\n        }\n        thread::sleep(Duration::from_millis(delay));\n        delay *= 2;\n    }\n}".to_string(),
                                start_line: 10,
                                end_line: 19,
                            }),
                            relevance: Some(0.9534),
                            distance: Some(0.0466),
                            similarity: Some(0.9534),
                        },
                        Node {
                            node_id: "node2".into(),
                            node: NodeData::FileChunk(forge_domain::FileChunk {
                                file_path: "src/http/client.rs".to_string(),
                                content: "async fn request_with_retry(&self, url: &str) -> Result<Response> {\n    const MAX_RETRIES: usize = 3;\n    let mut backoff = ExponentialBackoff::default();\n    // Implementation...\n}".to_string(),
                                start_line: 45,
                                end_line: 50,
                            }),
                            relevance: Some(0.9201),
                            distance: Some(0.0799),
                            similarity: Some(0.9201),
                        },
                    ],
                }],
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::SemSearch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_sem_search_with_usecase() {
        use forge_domain::{CodebaseQueryResult, CodebaseSearchResults, Node, NodeData};

        let fixture = ToolOperation::CodebaseSearch {
            output: CodebaseSearchResults {
                queries: vec![CodebaseQueryResult {
                    query: "authentication logic".to_string(),
                    use_case: "need to add similar auth to my endpoint".to_string(),
                    results: vec![Node {
                        node_id: "node1".into(),
                        node: NodeData::FileChunk(forge_domain::FileChunk {
                            file_path: "src/auth.rs".to_string(),
                            content: "fn authenticate_user(token: &str) -> Result<User> {\n    verify_jwt(token)\n}".to_string(),
                            start_line: 10,
                            end_line: 12,
                        }),
                        relevance: Some(0.95),
                        distance: Some(0.05),
                        similarity: Some(0.95),
                    }],
                }],
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::SemSearch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_follow_up_no_question() {
        let fixture = ToolOperation::FollowUp { output: None };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Followup,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_skill_operation() {
        let fixture = ToolOperation::Skill {
            input: forge_domain::SkillFetch { name: "test-skill".to_string() },
            output: forge_domain::Skill::new(
                "test-skill",
                "This is a test skill command with instructions",
                "A test skill for demonstration",
            )
            .path("/home/user/.forge/skills/test-skill")
            .resources(vec![
                PathBuf::from("/home/user/.forge/skills/test-skill/resource1.txt"),
                PathBuf::from("/home/user/.forge/skills/test-skill/resource2.md"),
            ]),
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Skill,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }
}
