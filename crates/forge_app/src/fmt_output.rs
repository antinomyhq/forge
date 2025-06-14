use forge_display::{DiffFormat, GrepFormat};
use forge_domain::Environment;

use crate::execution_result::ExecutionResult;
use crate::utils::format_match;

pub trait FormatOutput {
    fn to_content(&self, env: &Environment) -> Option<String>;
}

impl FormatOutput for ExecutionResult {
    fn to_content(&self, env: &Environment) -> Option<String> {
        match self {
            ExecutionResult::FsRead(_) => None,
            ExecutionResult::FsCreate(_) => None,
            ExecutionResult::FsRemove(_) => None,
            ExecutionResult::FsSearch(output) => output.as_ref().map(|result| {
                GrepFormat::new(
                    result
                        .matches
                        .iter()
                        .map(|match_| format_match(match_, env))
                        .collect::<Vec<_>>(),
                )
                .format()
            }),
            ExecutionResult::FsPatch(output) => {
                Some(DiffFormat::format(&output.before, &output.after))
            }
            ExecutionResult::FsUndo(_) => None,
            ExecutionResult::NetFetch(_) => None,
            ExecutionResult::Shell(_) => None,
            ExecutionResult::FollowUp(_) => None,
            ExecutionResult::AttemptCompletion => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use console::strip_ansi_codes;
    use forge_domain::Environment;
    use insta::assert_snapshot;
    use pretty_assertions::assert_eq;

    use super::FormatOutput;
    use crate::execution_result::ExecutionResult;
    use crate::{
        Content, FsCreateOutput, FsRemoveOutput, FsUndoOutput, HttpResponse, Match, MatchResult,
        PatchOutput, ReadOutput, ResponseContext, SearchResult, ShellOutput,
    };

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

    #[test]
    fn test_fs_read_single_line() {
        let fixture = ExecutionResult::FsRead(ReadOutput {
            content: Content::File("Hello, world!".to_string()),
            start_line: 1,
            end_line: 1,
            total_lines: 5,
        });
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_read_multiple_lines() {
        let fixture = ExecutionResult::FsRead(ReadOutput {
            content: Content::File("Line 1\nLine 2\nLine 3".to_string()),
            start_line: 2,
            end_line: 4,
            total_lines: 10,
        });
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_create_new_file() {
        let fixture = ExecutionResult::FsCreate(FsCreateOutput {
            path: "/home/user/project/new_file.txt".to_string(),
            before: None,
            warning: None,
        });
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_create_overwrite() {
        let fixture = ExecutionResult::FsCreate(FsCreateOutput {
            path: "/home/user/project/existing_file.txt".to_string(),
            before: Some("old content".to_string()),
            warning: None,
        });
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_create_with_warning() {
        let fixture = ExecutionResult::FsCreate(FsCreateOutput {
            path: "/home/user/project/file.txt".to_string(),
            before: None,
            warning: Some("File created outside project directory".to_string()),
        });
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_remove() {
        let fixture = ExecutionResult::FsRemove(FsRemoveOutput {});
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_search_with_matches() {
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
        let fixture = ExecutionResult::FsSearch(Some(SearchResult {
            matches: vec![Match {
                path: "file1.txt".to_string(),
                result: Some(MatchResult::Error("Permission denied".to_string())),
            }],
        }));
        let env = fixture_environment();

        let actual = fixture.to_content(&env);

        // Should return Some(String) with formatted grep output even for errors
        assert!(actual.is_some());
        let output = actual.unwrap();
        assert!(output.contains("file1.txt"));
    }

    #[test]
    fn test_fs_search_none() {
        let fixture = ExecutionResult::FsSearch(None);
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_patch_success() {
        let fixture = ExecutionResult::FsPatch(PatchOutput {
            warning: None,
            before: "Hello world\nThis is a test".to_string(),
            after: "Hello universe\nThis is a test\nNew line".to_string(),
        });
        let env = fixture_environment();
        let actual = fixture.to_content(&env).unwrap();
        let actual = strip_ansi_codes(actual.as_str());
        assert_snapshot!(actual)
    }

    #[test]
    fn test_fs_patch_with_warning() {
        let fixture = ExecutionResult::FsPatch(PatchOutput {
            warning: Some("Large file modification".to_string()),
            before: "line1\nline2".to_string(),
            after: "line1\nnew line\nline2".to_string(),
        });
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
        let fixture = ExecutionResult::FsUndo(FsUndoOutput {
            before_undo: Some("ABC".to_string()),
            after_undo: Some("PQR".to_string()),
        });
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_net_fetch_success() {
        let fixture = ExecutionResult::NetFetch(HttpResponse {
            content: "# Example Website\n\nThis is content.".to_string(),
            code: 200,
            context: ResponseContext::Parsed,
            content_type: "text/html".to_string(),
        });
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_net_fetch_error() {
        let fixture = ExecutionResult::NetFetch(HttpResponse {
            content: "Not Found".to_string(),
            code: 404,
            context: ResponseContext::Raw,
            content_type: "text/plain".to_string(),
        });
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_shell_success() {
        let fixture = ExecutionResult::Shell(ShellOutput {
            output: forge_domain::CommandOutput {
                command: "ls -la".to_string(),
                stdout: "file1.txt\nfile2.txt".to_string(),
                stderr: "".to_string(),
                exit_code: Some(0),
            },
            shell: "/bin/bash".to_string(),
        });
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_shell_success_with_stderr() {
        let fixture = ExecutionResult::Shell(ShellOutput {
            output: forge_domain::CommandOutput {
                command: "command_with_warnings".to_string(),
                stdout: "output line".to_string(),
                stderr: "warning line".to_string(),
                exit_code: Some(0),
            },
            shell: "/bin/bash".to_string(),
        });
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_shell_failure() {
        let fixture = ExecutionResult::Shell(ShellOutput {
            output: forge_domain::CommandOutput {
                command: "failing_command".to_string(),
                stdout: "".to_string(),
                stderr: "Error: command not found".to_string(),
                exit_code: Some(127),
            },
            shell: "/bin/bash".to_string(),
        });
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_follow_up_with_response() {
        let fixture =
            ExecutionResult::FollowUp(Some("Yes, continue with the operation".to_string()));
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_follow_up_no_response() {
        let fixture = ExecutionResult::FollowUp(None);
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_attempt_completion() {
        let fixture = ExecutionResult::AttemptCompletion;
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }
}
