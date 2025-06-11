use std::path::{Path, PathBuf};

use console::strip_ansi_codes;
use forge_display::DiffFormat;
use forge_domain::{Environment, Tools};
use forge_template::Element;

use crate::front_matter::FrontMatter;
use crate::truncation::FETCH_MAX_LENGTH;
use crate::utils::display_path;
use crate::{
    Content, FetchOutput, FsCreateOutput, FsRemoveOutput, FsUndoOutput, PatchOutput, ReadOutput,
    SearchResult, Services, ShellOutput, create_temp_file, truncate_search_output,
};

#[derive(Debug, derive_more::From)]
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
        input: Tools,
        truncation_path: Option<PathBuf>,
        env: &Environment,
    ) -> anyhow::Result<forge_domain::ToolOutput> {
        match (input, self) {
            (Tools::ForgeToolFsRead(input), ExecutionResult::FsRead(out)) => match &out.content {
                Content::File(content) => {
                    let elm = Element::new("file_content")
                        .attr("path", input.path)
                        .attr("start_line", out.start_line)
                        .attr("end_line", out.end_line)
                        .attr("total_lines", content.lines().count())
                        .cdata(content);

                    Ok(forge_domain::ToolOutput::text(elm))
                }
            },
            (Tools::ForgeToolFsCreate(input), ExecutionResult::FsCreate(output)) => {
                let mut elm = if let Some(before) = output.previous {
                    let diff =
                        console::strip_ansi_codes(&DiffFormat::format(&before, &input.content))
                            .to_string();
                    Element::new("file_diff").cdata(diff)
                } else {
                    Element::new("file_content").cdata(&input.content)
                };

                elm = elm
                    .attr("path", input.path)
                    .attr("total_lines", input.content.lines().count());

                if let Some(warning) = output.warning {
                    elm = elm.append(Element::new("warning").text(warning));
                }

                Ok(forge_domain::ToolOutput::text(elm))
            }
            (Tools::ForgeToolFsRemove(input), ExecutionResult::FsRemove(output)) => {
                let display_path = display_path(env, Path::new(&input.path))?;
                let elm = if output.completed {
                    Element::new("file_removed")
                        .attr("path", display_path)
                        .attr("status", "success")
                } else {
                    Element::new("file_removed")
                        .attr("path", display_path)
                        .attr("status", "not_found")
                };

                Ok(forge_domain::ToolOutput::text(elm))
            }
            (Tools::ForgeToolFsSearch(input), ExecutionResult::FsSearch(output)) => {
                match output {
                    Some(output) => {
                        let truncated_output = truncate_search_output(
                            &output.matches,
                            &input.path,
                            input.regex.as_ref(),
                            input.file_pattern.as_ref(),
                        );
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
            }
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

                Ok(forge_domain::ToolOutput::text(elm))
            }
            (Tools::ForgeToolFsUndo(input), ExecutionResult::FsUndo(output)) => {
                let diff = DiffFormat::format(&output.before_undo, &output.after_undo);
                let elm = Element::new("file_diff")
                    .attr("path", input.path)
                    .attr("status", "restored")
                    .cdata(strip_ansi_codes(&diff));

                Ok(forge_domain::ToolOutput::text(elm))
            }
            (Tools::ForgeToolNetFetch(input), ExecutionResult::NetFetch(output)) => {
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
            }
            (_, ExecutionResult::Shell(output)) => {
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
            (_, ExecutionResult::FollowUp(output)) => match output {
                None => {
                    let elm = Element::new("interrupted").text("No feedback provided");
                    Ok(forge_domain::ToolOutput::text(elm))
                }
                Some(content) => {
                    let elm = Element::new("feedback").text(content);
                    Ok(forge_domain::ToolOutput::text(elm))
                }
            },
            (_, ExecutionResult::AttemptCompletion) => Ok(forge_domain::ToolOutput::text(
                "[Task was completed successfully. Now wait for user feedback]".to_string(),
            )),
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
            ExecutionResult::NetFetch(output) => {
                let original_length = output.content.len();
                let is_truncated = original_length > crate::truncation::FETCH_MAX_LENGTH;

                if is_truncated {
                    let path =
                        create_temp_file(services, "forge_fetch_", ".txt", &output.content).await?;

                    Ok(Some(path))
                } else {
                    Ok(None)
                }
            }
            ExecutionResult::Shell(output) => {
                let stdout_lines = output.output.stdout.lines().count();
                let stderr_lines = output.output.stderr.lines().count();
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

#[cfg(test)]
mod tests {
    use std::fmt::Write;
    use std::path::PathBuf;

    use forge_domain::{FSRead, ToolValue, Tools};

    use super::*;

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

        let actual = fixture.into_tool_output(input, None, &env).unwrap();

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

        let actual = fixture.into_tool_output(input, None, &env).unwrap();

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

        let actual = fixture.into_tool_output(input, None, &env).unwrap();

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

        let actual = fixture
            .into_tool_output(input, truncation_path, &env)
            .unwrap();

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_create_basic() {
        let fixture = ExecutionResult::FsCreate(FsCreateOutput {
            path: "/home/user/new_file.txt".to_string(),
            previous: None,
            warning: None,
        });

        let input = Tools::ForgeToolFsCreate(forge_domain::FSWrite {
            path: "/home/user/new_file.txt".to_string(),
            content: "Hello, world!".to_string(),
            overwrite: false,
            explanation: Some("Creating a new file".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env).unwrap();

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_create_overwrite() {
        let fixture = ExecutionResult::FsCreate(FsCreateOutput {
            path: "/home/user/existing_file.txt".to_string(),
            previous: Some("Old content".to_string()),
            warning: None,
        });

        let input = Tools::ForgeToolFsCreate(forge_domain::FSWrite {
            path: "/home/user/existing_file.txt".to_string(),
            content: "New content for the file".to_string(),
            overwrite: true,
            explanation: Some("Overwriting existing file".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env).unwrap();

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_create_with_warning() {
        let fixture = ExecutionResult::FsCreate(FsCreateOutput {
            path: "/home/user/file_with_warning.txt".to_string(),
            previous: None,
            warning: Some("File created in non-standard location".to_string()),
        });

        let input = Tools::ForgeToolFsCreate(forge_domain::FSWrite {
            path: "/home/user/file_with_warning.txt".to_string(),
            content: "Content with warning".to_string(),
            overwrite: false,
            explanation: Some("Creating file with warning".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env).unwrap();

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_remove_success() {
        let fixture = ExecutionResult::FsRemove(FsRemoveOutput { completed: true });

        let input = Tools::ForgeToolFsRemove(forge_domain::FSRemove {
            path: "/home/user/file_to_delete.txt".to_string(),
            explanation: Some("Removing unnecessary file".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env).unwrap();

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_remove_not_found() {
        let fixture = ExecutionResult::FsRemove(FsRemoveOutput { completed: false });

        let input = Tools::ForgeToolFsRemove(forge_domain::FSRemove {
            path: "/home/user/nonexistent_file.txt".to_string(),
            explanation: Some("Trying to remove file that doesn't exist".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env).unwrap();

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_with_results() {
        let fixture = ExecutionResult::FsSearch(Some(SearchResult {
            matches: vec![
                "file1.txt:1:Hello world".to_string(),
                "file2.txt:3:Hello universe".to_string(),
            ],
        }));

        let input = Tools::ForgeToolFsSearch(forge_domain::FSSearch {
            path: "/home/user/project".to_string(),
            regex: Some("Hello".to_string()),
            file_pattern: Some("*.txt".to_string()),
            explanation: Some("Searching for Hello pattern".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env).unwrap();

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_no_results() {
        let fixture = ExecutionResult::FsSearch(None);

        let input = Tools::ForgeToolFsSearch(forge_domain::FSSearch {
            path: "/home/user/project".to_string(),
            regex: Some("NonExistentPattern".to_string()),
            file_pattern: None,
            explanation: Some("Searching for non-existent pattern".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env).unwrap();

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
            search: "world".to_string(),
            operation: forge_domain::PatchOperation::Replace,
            content: "universe".to_string(),
            explanation: Some("Replacing world with universe".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env).unwrap();

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
            search: "line1".to_string(),
            operation: forge_domain::PatchOperation::Append,
            content: "\nnew line".to_string(),
            explanation: Some("Adding new line after line1".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env).unwrap();

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_undo_success() {
        let fixture = ExecutionResult::FsUndo(FsUndoOutput {
            before_undo: "ABC".to_string(),
            after_undo: "PQR".to_string(),
        });

        let input = Tools::ForgeToolFsUndo(forge_domain::FSUndo {
            path: "/home/user/test.txt".to_string(),
            explanation: Some("Reverting changes to test file".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env).unwrap();

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_net_fetch_success() {
        let fixture = ExecutionResult::NetFetch(FetchOutput {
            content: "# Example Website\n\nThis is some content from a website.".to_string(),
            code: 200,
            context: "https://example.com".to_string(),
        });

        let input = Tools::ForgeToolNetFetch(forge_domain::NetFetch {
            url: "https://example.com".to_string(),
            raw: Some(false),
            explanation: Some("Fetching content from example website".to_string()),
        });

        let env = fixture_environment();

        let actual = fixture.into_tool_output(input, None, &env).unwrap();

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

        let actual = fixture.into_tool_output(input, None, &env).unwrap();

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

        let actual = fixture.into_tool_output(input, None, &env).unwrap();

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

        let actual = fixture.into_tool_output(input, None, &env).unwrap();

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
