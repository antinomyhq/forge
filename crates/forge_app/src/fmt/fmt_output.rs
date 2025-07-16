use forge_display::{DiffFormat, GrepFormat};
use forge_domain::Environment;

use crate::fmt::content::{ContentFormat, FormatContent};
use crate::operation::Operation;
use crate::utils::format_match;

impl FormatContent for Operation {
    fn to_content(&self, env: &Environment) -> Option<ContentFormat> {
        match self {
            Operation::FsRead { input: _, output: _ } => None,
            Operation::FsCreate { input: _, output: _ } => None,
            Operation::FsRemove { input: _ } => None,
            Operation::FsSearch { input: _, output } => output.as_ref().map(|result| {
                ContentFormat::PlainText(
                    GrepFormat::new(
                        result
                            .matches
                            .iter()
                            .map(|match_| format_match(match_, env))
                            .collect::<Vec<_>>(),
                    )
                    .format(),
                )
            }),
            Operation::FsPatch { input: _, output } => Some(ContentFormat::PlainText(
                DiffFormat::format(&output.before, &output.after),
            )),
            Operation::FsUndo { input: _, output: _ } => None,
            Operation::NetFetch { input: _, output: _ } => None,
            Operation::Shell { output: _ } => None,
            Operation::FollowUp { output: _ } => None,
            Operation::AttemptCompletion => None,
            Operation::TaskListAppend { _input: _, before, after }
            | Operation::TaskListAppendMultiple { _input: _, before, after }
            | Operation::TaskListUpdate { _input: _, before, after }
            | Operation::TaskListList { _input: _, before, after }
            | Operation::TaskListClear { _input: _, before, after } => Some(
                ContentFormat::Markdown(crate::fmt::fmt_task::to_markdown(before, after)),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use console::strip_ansi_codes;
    use forge_domain::{Environment, PatchOperation};
    use insta::assert_snapshot;
    use pretty_assertions::assert_eq;
    use url::Url;

    use super::FormatContent;
    use crate::fmt::content::ContentFormat;
    use crate::operation::Operation;
    use crate::{
        Content, FsCreateOutput, FsUndoOutput, HttpResponse, Match, MatchResult, PatchOutput,
        ReadOutput, ResponseContext, SearchResult, ShellOutput,
    };

    impl std::fmt::Display for ContentFormat {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                ContentFormat::Title(title) => write!(f, "{title}"),
                ContentFormat::PlainText(text) => write!(f, "{text}"),
                ContentFormat::Markdown(text) => write!(f, "{text}"),
            }
        }
    }

    impl ContentFormat {
        pub fn contains(&self, needle: &str) -> bool {
            self.to_string().contains(needle)
        }

        pub fn as_str(&self) -> &str {
            match self {
                ContentFormat::PlainText(text) | ContentFormat::Markdown(text) => text,
                ContentFormat::Title(_) => {
                    // For titles, we can't return a reference to the formatted string
                    // since it's computed on demand. Tests should use to_string() instead.
                    panic!("as_str() not supported for Title format, use to_string() instead")
                }
            }
        }
    }

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
            max_file_size: 0,
            cert: None,
            forge_api_url: Url::parse("http://forgecode.dev/api").unwrap(),
        }
    }

    #[test]
    fn test_fs_read_single_line() {
        let fixture = Operation::FsRead {
            input: forge_domain::FSRead {
                path: "/home/user/test.txt".to_string(),
                start_line: None,
                end_line: None,
                explanation: Some("Test explanation".to_string()),
            },
            output: ReadOutput {
                content: Content::File("Hello, world!".to_string()),
                start_line: 1,
                end_line: 1,
                total_lines: 5,
            },
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_read_multiple_lines() {
        let fixture = Operation::FsRead {
            input: forge_domain::FSRead {
                path: "/home/user/test.txt".to_string(),
                start_line: Some(2),
                end_line: Some(4),
                explanation: Some("Test explanation".to_string()),
            },
            output: ReadOutput {
                content: Content::File("Line 1\nLine 2\nLine 3".to_string()),
                start_line: 2,
                end_line: 4,
                total_lines: 10,
            },
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_create_new_file() {
        let fixture = Operation::FsCreate {
            input: forge_domain::FSWrite {
                path: "/home/user/project/new_file.txt".to_string(),
                content: "New file content".to_string(),
                overwrite: false,
                explanation: Some("Create new file".to_string()),
            },
            output: FsCreateOutput {
                path: "/home/user/project/new_file.txt".to_string(),
                before: None,
                warning: None,
            },
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_create_overwrite() {
        let fixture = Operation::FsCreate {
            input: forge_domain::FSWrite {
                path: "/home/user/project/existing_file.txt".to_string(),
                content: "new content".to_string(),
                overwrite: true,
                explanation: Some("Overwrite existing file".to_string()),
            },
            output: FsCreateOutput {
                path: "/home/user/project/existing_file.txt".to_string(),
                before: Some("old content".to_string()),
                warning: None,
            },
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_create_with_warning() {
        let fixture = Operation::FsCreate {
            input: forge_domain::FSWrite {
                path: "/home/user/project/file.txt".to_string(),
                content: "File content".to_string(),
                overwrite: false,
                explanation: Some("Create file".to_string()),
            },
            output: FsCreateOutput {
                path: "/home/user/project/file.txt".to_string(),
                before: None,
                warning: Some("File created outside project directory".to_string()),
            },
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_remove() {
        let fixture = Operation::FsRemove {
            input: forge_domain::FSRemove {
                path: "/home/user/project/file.txt".to_string(),
                explanation: Some("Remove file".to_string()),
            },
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_search_with_matches() {
        let fixture = Operation::FsSearch {
            input: forge_domain::FSSearch {
                path: "/home/user/project".to_string(),
                regex: Some("Hello".to_string()),
                file_pattern: None,
                max_search_lines: None,
                start_index: None,
                explanation: Some("Search for Hello".to_string()),
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

        let actual = fixture.to_content(&env);

        // Should return Some(String) with formatted grep output
        assert!(actual.is_some());
        let output = actual.unwrap();
        assert!(output.contains("file1.txt"));
        assert!(output.contains("Hello world"));
        assert!(output.contains("file2.txt"));
        assert!(output.contains("Hello universe"));
    }

    #[test]
    fn test_fs_search_no_matches() {
        let fixture = Operation::FsSearch {
            input: forge_domain::FSSearch {
                path: "/home/user/project".to_string(),
                regex: Some("nonexistent".to_string()),
                file_pattern: None,
                max_search_lines: None,
                start_index: None,
                explanation: Some("Search for nonexistent".to_string()),
            },
            output: Some(SearchResult {
                matches: vec![Match {
                    path: "file1.txt".to_string(),
                    result: Some(MatchResult::Error("Permission denied".to_string())),
                }],
            }),
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);

        // Should return Some(String) with formatted grep output even for errors
        assert!(actual.is_some());
        let output = actual.unwrap();
        assert!(output.contains("file1.txt"));
    }

    #[test]
    fn test_fs_search_none() {
        let fixture = Operation::FsSearch {
            input: forge_domain::FSSearch {
                path: "/home/user/project".to_string(),
                regex: Some("search".to_string()),
                file_pattern: None,
                max_search_lines: None,
                start_index: None,
                explanation: Some("Search test".to_string()),
            },
            output: None,
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_patch_success() {
        let fixture = Operation::FsPatch {
            input: forge_domain::FSPatch {
                path: "/home/user/project/test.txt".to_string(),
                search: Some("Hello world".to_string()),
                content: "Hello universe".to_string(),
                operation: PatchOperation::Replace,
                explanation: Some("Replace text".to_string()),
            },
            output: PatchOutput {
                warning: None,
                before: "Hello world\nThis is a test".to_string(),
                after: "Hello universe\nThis is a test\nNew line".to_string(),
            },
        };
        let env = fixture_environment();
        let actual = fixture.to_content(&env).unwrap();
        let actual = strip_ansi_codes(actual.as_str());
        assert_snapshot!(actual)
    }

    #[test]
    fn test_fs_patch_with_warning() {
        let fixture = Operation::FsPatch {
            input: forge_domain::FSPatch {
                path: "/home/user/project/large_file.txt".to_string(),
                search: Some("line2".to_string()),
                content: "new line\nline2".to_string(),
                operation: PatchOperation::Replace,
                explanation: Some("Add new line".to_string()),
            },
            output: PatchOutput {
                warning: Some("Large file modification".to_string()),
                before: "line1\nline2".to_string(),
                after: "line1\nnew line\nline2".to_string(),
            },
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);

        // Should return Some(String) with formatted diff output
        assert!(actual.is_some());
        let output = actual.unwrap();
        assert!(output.contains("line1"));
        assert!(output.contains("new line"));
    }

    #[test]
    fn test_fs_undo() {
        let fixture = Operation::FsUndo {
            input: forge_domain::FSUndo {
                path: "/home/user/project/test.txt".to_string(),
                explanation: Some("Undo changes".to_string()),
            },
            output: FsUndoOutput {
                before_undo: Some("ABC".to_string()),
                after_undo: Some("PQR".to_string()),
            },
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_net_fetch_success() {
        let fixture = Operation::NetFetch {
            input: forge_domain::NetFetch {
                url: "https://example.com".to_string(),
                raw: Some(false),
                explanation: Some("Fetch example website".to_string()),
            },
            output: HttpResponse {
                content: "# Example Website\n\nThis is content.".to_string(),
                code: 200,
                context: ResponseContext::Parsed,
                content_type: "text/html".to_string(),
            },
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_net_fetch_error() {
        let fixture = Operation::NetFetch {
            input: forge_domain::NetFetch {
                url: "https://example.com/notfound".to_string(),
                raw: Some(true),
                explanation: Some("Fetch non-existent page".to_string()),
            },
            output: HttpResponse {
                content: "Not Found".to_string(),
                code: 404,
                context: ResponseContext::Raw,
                content_type: "text/plain".to_string(),
            },
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_shell_success() {
        let fixture = Operation::Shell {
            output: ShellOutput {
                output: forge_domain::CommandOutput {
                    command: "ls -la".to_string(),
                    stdout: "file1.txt\nfile2.txt".to_string(),
                    stderr: "".to_string(),
                    exit_code: Some(0),
                },
                shell: "/bin/bash".to_string(),
            },
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_shell_success_with_stderr() {
        let fixture = Operation::Shell {
            output: ShellOutput {
                output: forge_domain::CommandOutput {
                    command: "command_with_warnings".to_string(),
                    stdout: "output line".to_string(),
                    stderr: "warning line".to_string(),
                    exit_code: Some(0),
                },
                shell: "/bin/bash".to_string(),
            },
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_shell_failure() {
        let fixture = Operation::Shell {
            output: ShellOutput {
                output: forge_domain::CommandOutput {
                    command: "failing_command".to_string(),
                    stdout: "".to_string(),
                    stderr: "Error: command not found".to_string(),
                    exit_code: Some(127),
                },
                shell: "/bin/bash".to_string(),
            },
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_follow_up_with_response() {
        let fixture =
            Operation::FollowUp { output: Some("Yes, continue with the operation".to_string()) };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_follow_up_no_response() {
        let fixture = Operation::FollowUp { output: None };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_attempt_completion() {
        let fixture = Operation::AttemptCompletion;
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }
}
