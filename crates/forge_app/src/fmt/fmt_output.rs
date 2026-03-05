use forge_display::DiffFormat;
use forge_domain::{ChatResponseContent, Environment, TitleFormat};

use crate::fmt::content::FormatContent;
use crate::operation::ToolOperation;
use crate::utils::format_display_path;

impl FormatContent for ToolOperation {
    fn to_content(&self, env: &Environment) -> Option<ChatResponseContent> {
        match self {
            ToolOperation::FsWrite { input, output } => {
                if let Some(ref before) = output.before {
                    let after = &input.content;
                    Some(ChatResponseContent::ToolOutput(
                        DiffFormat::format(before, after).diff().to_string(),
                    ))
                } else {
                    None
                }
            }
            ToolOperation::FsPatch { input: _, output } => Some(ChatResponseContent::ToolOutput(
                DiffFormat::format(&output.before, &output.after)
                    .diff()
                    .to_string(),
            )),
            ToolOperation::PlanCreate { input: _, output } => Some({
                let title = TitleFormat::debug(format!(
                    "Create {}",
                    format_display_path(&output.path, &env.cwd)
                ));
                title.into()
            }),
            ToolOperation::TodoWrite { before, after } => Some(ChatResponseContent::ToolOutput(
                format_todos_diff(before, after),
            )),
            ToolOperation::TodoRead { output } => {
                Some(ChatResponseContent::ToolOutput(format_todos(output)))
            }
            ToolOperation::FsRead { input: _, output: _ }
            | ToolOperation::FsRemove { input: _, output: _ }
            | ToolOperation::FsSearch { input: _, output: _ }
            | ToolOperation::CodebaseSearch { output: _ }
            | ToolOperation::FsUndo { input: _, output: _ }
            | ToolOperation::NetFetch { input: _, output: _ }
            | ToolOperation::Shell { output: _ }
            | ToolOperation::FollowUp { output: _ }
            | ToolOperation::Skill { output: _ } => None,
        }
    }
}

/// Controls the styling applied to a rendered todo line.
enum TodoLineStyle {
    /// Bold — used for new or changed todos. Color is determined by todo
    /// status.
    Bold,
    /// Dim — used for unchanged todos. Color is determined by todo status.
    Dim,
}

/// Renders a single todo as an indented line with icon and ANSI styling.
///
/// Color is always driven by the todo's current status:
/// - Pending → white
/// - InProgress → cyan
/// - Completed → green (content also gets strikethrough)
///
/// Emphasis is driven by `line_style`: bold for new/changed, dim for unchanged.
fn format_todo_line(todo: &forge_domain::Todo, line_style: TodoLineStyle) -> String {
    use console::style;
    use forge_domain::TodoStatus;

    let checkbox = match todo.status {
        TodoStatus::Completed => "󰄵",
        TodoStatus::InProgress => "󰄗",
        TodoStatus::Pending => "󰄱",
    };

    let content = match todo.status {
        TodoStatus::Completed => style(todo.content.as_str()).strikethrough().to_string(),
        _ => todo.content.clone(),
    };

    let line = format!("  {checkbox} {content}");
    let styled = match (&todo.status, line_style) {
        (TodoStatus::Pending, TodoLineStyle::Bold) => style(line).white().bold().to_string(),
        (TodoStatus::Pending, TodoLineStyle::Dim) => style(line).white().dim().to_string(),
        (TodoStatus::InProgress, TodoLineStyle::Bold) => style(line).cyan().bold().to_string(),
        (TodoStatus::InProgress, TodoLineStyle::Dim) => style(line).cyan().dim().to_string(),
        (TodoStatus::Completed, TodoLineStyle::Bold) => style(line).green().bold().to_string(),
        (TodoStatus::Completed, TodoLineStyle::Dim) => style(line).green().dim().to_string(),
    };
    format!("{styled}\n")
}

/// Formats a todo diff showing only what changed between before and after
fn format_todos_diff(before: &[forge_domain::Todo], after: &[forge_domain::Todo]) -> String {
    use console::style;

    let before_map: std::collections::HashMap<&str, &forge_domain::Todo> =
        before.iter().map(|t| (t.id.as_str(), t)).collect();
    let after_ids: std::collections::HashSet<&str> = after.iter().map(|t| t.id.as_str()).collect();

    let mut result = String::new();

    // All todos in the new list — highlight changes, dim unchanged
    for todo in after {
        let prev = before_map.get(todo.id.as_str()).copied();
        let is_new = prev.is_none();
        let is_changed = prev
            .map(|p| p.status != todo.status || p.content != todo.content)
            .unwrap_or(false);

        let line_style = if is_new || is_changed {
            TodoLineStyle::Bold
        } else {
            TodoLineStyle::Dim
        };
        result.push_str(&format_todo_line(todo, line_style));
    }
    // Removed todos (show as cancelled with strikethrough + yellow)
    for todo in before {
        if !after_ids.contains(todo.id.as_str()) {
            let content = style(todo.content.as_str()).strikethrough().to_string();
            result.push_str(&format!(
                "{}\n",
                style(format!("\u{f057} {content}")).yellow()
            ));
        }
    }

    result
}
/// Formats todos as markdown-style checkboxes
fn format_todos(todos: &[forge_domain::Todo]) -> String {
    if todos.is_empty() {
        return String::new();
    }

    let mut result = String::new();

    for todo in todos {
        result.push_str(&format_todo_line(todo, TodoLineStyle::Dim));
    }
    result
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use console::strip_ansi_codes;
    use forge_display::DiffFormat;
    use forge_domain::{ChatResponseContent, Environment};
    use insta::assert_snapshot;
    use pretty_assertions::assert_eq;

    use super::FormatContent;
    // ContentFormat is now ChatResponseContent
    use crate::operation::ToolOperation;
    use crate::{
        Content, FsRemoveOutput, FsUndoOutput, FsWriteOutput, HttpResponse, Match, MatchResult,
        PatchOutput, ReadOutput, ResponseContext, SearchResult, ShellOutput,
    };

    // ContentFormat methods are now implemented in ChatResponseContent

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
            .max_line_length(100)
            .max_file_size(0)
    }

    #[test]
    fn test_fs_read_single_line() {
        let content = "Hello, world!";
        let fixture = ToolOperation::FsRead {
            input: forge_domain::FSRead {
                file_path: "/home/user/test.txt".to_string(),
                start_line: None,
                end_line: None,
                show_line_numbers: true,
            },
            output: ReadOutput {
                content: Content::file(content),
                start_line: 1,
                end_line: 1,
                total_lines: 5,
                content_hash: crate::compute_hash(content),
            },
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_read_multiple_lines() {
        let content = "Line 1\nLine 2\nLine 3";
        let fixture = ToolOperation::FsRead {
            input: forge_domain::FSRead {
                file_path: "/home/user/test.txt".to_string(),
                start_line: Some(2),
                end_line: Some(4),
                show_line_numbers: true,
            },
            output: ReadOutput {
                content: Content::file(content),
                start_line: 2,
                end_line: 4,
                total_lines: 10,
                content_hash: crate::compute_hash(content),
            },
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_create_new_file() {
        let content = "New file content";
        let fixture = ToolOperation::FsWrite {
            input: forge_domain::FSWrite {
                file_path: "/home/user/project/new_file.txt".to_string(),
                content: content.to_string(),
                overwrite: false,
            },
            output: FsWriteOutput {
                path: "/home/user/project/new_file.txt".to_string(),
                before: None,
                errors: vec![],
                content_hash: crate::compute_hash(content),
            },
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_create_overwrite() {
        let content = "new content";
        let fixture = ToolOperation::FsWrite {
            input: forge_domain::FSWrite {
                file_path: "/home/user/project/existing_file.txt".to_string(),
                content: content.to_string(),
                overwrite: true,
            },
            output: FsWriteOutput {
                path: "/home/user/project/existing_file.txt".to_string(),
                before: Some("old content".to_string()),
                errors: vec![],
                content_hash: crate::compute_hash(content),
            },
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = Some(ChatResponseContent::ToolOutput(
            DiffFormat::format("old content", "new content")
                .diff()
                .to_string(),
        ));

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_create_with_warning() {
        let content = "File content";
        let fixture = ToolOperation::FsWrite {
            input: forge_domain::FSWrite {
                file_path: "/home/user/project/file.txt".to_string(),
                content: content.to_string(),
                overwrite: false,
            },
            output: FsWriteOutput {
                path: "/home/user/project/file.txt".to_string(),
                before: None,
                errors: vec![forge_domain::SyntaxError {
                    line: 5,
                    column: 10,
                    message: "Syntax error".to_string(),
                }],
                content_hash: crate::compute_hash(content),
            },
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_remove() {
        let fixture = ToolOperation::FsRemove {
            input: forge_domain::FSRemove { path: "/home/user/project/file.txt".to_string() },
            output: FsRemoveOutput { content: "".to_string() },
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_search_with_matches() {
        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: Some("/home/user/project".to_string()),
                pattern: "Hello".to_string(),
                ..Default::default()
            },
            output: Some(SearchResult {
                matches: vec![
                    Match {
                        path: "file1.txt".to_string(),
                        result: Some(MatchResult::Found {
                            line_number: Some(1),
                            line: "Hello world".to_string(),
                        }),
                    },
                    Match {
                        path: "file2.txt".to_string(),
                        result: Some(MatchResult::Found {
                            line_number: Some(3),
                            line: "Hello universe".to_string(),
                        }),
                    },
                ],
            }),
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_search_no_matches() {
        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: Some("/home/user/project".to_string()),
                pattern: "nonexistent".to_string(),
                ..Default::default()
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
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_fs_search_none() {
        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: Some("/home/user/project".to_string()),
                pattern: "search".to_string(),
                ..Default::default()
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
        let after_content = "Hello universe\nThis is a test\nNew line";
        let fixture = ToolOperation::FsPatch {
            input: forge_domain::FSPatch {
                file_path: "/home/user/project/test.txt".to_string(),
                old_string: "Hello world".to_string(),
                new_string: "Hello universe".to_string(),
                replace_all: false,
            },
            output: PatchOutput {
                errors: vec![],
                before: "Hello world\nThis is a test".to_string(),
                after: after_content.to_string(),
                content_hash: crate::compute_hash(after_content),
            },
        };
        let env = fixture_environment();
        let actual = fixture.to_content(&env).unwrap();
        let actual = strip_ansi_codes(actual.as_str());
        assert_snapshot!(actual)
    }

    #[test]
    fn test_fs_patch_with_warning() {
        let after_content = "line1\nnew line\nline2";
        let fixture = ToolOperation::FsPatch {
            input: forge_domain::FSPatch {
                file_path: "/home/user/project/large_file.txt".to_string(),
                old_string: "line2".to_string(),
                new_string: "new line\nline2".to_string(),
                replace_all: false,
            },
            output: PatchOutput {
                errors: vec![forge_domain::SyntaxError {
                    line: 10,
                    column: 5,
                    message: "Syntax error".to_string(),
                }],
                before: "line1\nline2".to_string(),
                after: after_content.to_string(),
                content_hash: crate::compute_hash(after_content),
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
        let fixture = ToolOperation::FsUndo {
            input: forge_domain::FSUndo { path: "/home/user/project/test.txt".to_string() },
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
        let fixture = ToolOperation::NetFetch {
            input: forge_domain::NetFetch {
                url: "https://example.com".to_string(),
                raw: Some(false),
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
        let fixture = ToolOperation::NetFetch {
            input: forge_domain::NetFetch {
                url: "https://example.com/notfound".to_string(),
                raw: Some(true),
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
        let fixture = ToolOperation::Shell {
            output: ShellOutput {
                output: forge_domain::CommandOutput {
                    command: "ls -la".to_string(),
                    stdout: "file1.txt\nfile2.txt".to_string(),
                    stderr: "".to_string(),
                    exit_code: Some(0),
                },
                shell: "/bin/bash".to_string(),
                description: None,
            },
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_shell_success_with_stderr() {
        let fixture = ToolOperation::Shell {
            output: ShellOutput {
                output: forge_domain::CommandOutput {
                    command: "command_with_warnings".to_string(),
                    stdout: "output line".to_string(),
                    stderr: "warning line".to_string(),
                    exit_code: Some(0),
                },
                shell: "/bin/bash".to_string(),
                description: None,
            },
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_shell_failure() {
        let fixture = ToolOperation::Shell {
            output: ShellOutput {
                output: forge_domain::CommandOutput {
                    command: "failing_command".to_string(),
                    stdout: "".to_string(),
                    stderr: "Error: command not found".to_string(),
                    exit_code: Some(127),
                },
                shell: "/bin/bash".to_string(),
                description: None,
            },
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_follow_up_with_response() {
        let fixture = ToolOperation::FollowUp {
            output: Some("Yes, continue with the operation".to_string()),
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_follow_up_no_response() {
        let fixture = ToolOperation::FollowUp { output: None };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_plan_create() {
        let fixture = ToolOperation::PlanCreate {
            input: forge_domain::PlanCreate {
                plan_name: "test-plan".to_string(),
                version: "v1".to_string(),
                content:
                    "# Test Plan\n\n## Task 1\n- Do something\n\n## Task 2\n- Do something else"
                        .to_string(),
            },
            output: crate::PlanCreateOutput {
                path: PathBuf::from("plans/2024-08-11-test-plan-v1.md"),
                before: None,
            },
        };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        if let Some(ChatResponseContent::ToolInput(title)) = actual {
            assert_eq!(title.title, "Create plans/2024-08-11-test-plan-v1.md");
            assert_eq!(title.category, forge_domain::Category::Debug);
            assert_eq!(title.sub_title, None);
        } else {
            panic!("Expected Title content");
        }
    }

    #[test]
    fn test_todo_write_empty() {
        let fixture = ToolOperation::TodoWrite { before: vec![], after: vec![] };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        assert!(actual.is_some());
        if let Some(ChatResponseContent::ToolOutput(text)) = actual {
            assert_eq!(text, "");
        } else {
            panic!("Expected ToolOutput content");
        }
    }

    #[test]
    fn test_todo_write_all_new_todos() {
        use forge_domain::{Todo, TodoStatus};

        // All todos are new (no before), so all should appear in diff
        let after = vec![
            Todo::new("Task 1").id("1").status(TodoStatus::Pending),
            Todo::new("Task 2").id("2").status(TodoStatus::InProgress),
            Todo::new("Task 3").id("3").status(TodoStatus::Completed),
        ];

        let fixture = ToolOperation::TodoWrite { before: vec![], after };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        assert!(actual.is_some());
        if let Some(ChatResponseContent::ToolOutput(text)) = actual {
            let plain = strip_ansi_codes(text.as_str());
            assert!(plain.contains("Task 1"));
            assert!(plain.contains("Task 2"));
            assert!(plain.contains("Task 3"));
        } else {
            panic!("Expected ToolOutput content");
        }
    }

    #[test]
    fn test_todo_write_unchanged_todos_shown() {
        use forge_domain::{Todo, TodoStatus};

        // Task 1 unchanged, Task 2 status changes: both should be shown
        let before = vec![
            Todo::new("Task 1").id("1").status(TodoStatus::Pending),
            Todo::new("Task 2").id("2").status(TodoStatus::Pending),
        ];
        let after = vec![
            Todo::new("Task 1").id("1").status(TodoStatus::Pending),
            Todo::new("Task 2").id("2").status(TodoStatus::Completed),
        ];

        let fixture = ToolOperation::TodoWrite { before, after };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        assert!(actual.is_some());
        if let Some(ChatResponseContent::ToolOutput(text)) = actual {
            let plain = strip_ansi_codes(text.as_str());
            assert!(plain.contains("Task 1"), "Unchanged Task 1 should appear");
            assert!(plain.contains("Task 2"), "Changed Task 2 should appear");
        } else {
            panic!("Expected ToolOutput content");
        }
    }

    #[test]
    fn test_todo_write_removed_todo_shown() {
        use forge_domain::{Todo, TodoStatus};

        // Task 2 removed
        let before = vec![
            Todo::new("Task 1").id("1").status(TodoStatus::Pending),
            Todo::new("Task 2").id("2").status(TodoStatus::InProgress),
        ];
        let after = vec![Todo::new("Task 1").id("1").status(TodoStatus::Pending)];

        let fixture = ToolOperation::TodoWrite { before, after };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        assert!(actual.is_some());
        if let Some(ChatResponseContent::ToolOutput(text)) = actual {
            let plain = strip_ansi_codes(text.as_str());
            assert!(plain.contains("Task 1"), "Unchanged Task 1 should appear");
            assert!(plain.contains("Task 2"), "Removed Task 2 should appear");
            assert!(
                plain.contains('\u{f057}'),
                "Removed task icon should appear"
            );
        } else {
            panic!("Expected ToolOutput content");
        }
    }

    // ANSI escape sequences emitted by console::style (verified against the
    // library):   bold white  → \x1b[37m\x1b[1m
    //   dim  white  → \x1b[37m\x1b[2m
    //   bold cyan   → \x1b[36m\x1b[1m
    //   dim  cyan   → \x1b[36m\x1b[2m
    //   bold green  → \x1b[32m\x1b[1m
    //   dim  green  → \x1b[32m\x1b[2m
    //   yellow      → \x1b[33m
    const BOLD_WHITE: &str = "\x1b[37m\x1b[1m";
    const DIM_WHITE: &str = "\x1b[37m\x1b[2m";
    const BOLD_CYAN: &str = "\x1b[36m\x1b[1m";
    const DIM_CYAN: &str = "\x1b[36m\x1b[2m";
    const BOLD_GREEN: &str = "\x1b[32m\x1b[1m";
    const DIM_GREEN: &str = "\x1b[32m\x1b[2m";
    const YELLOW: &str = "\x1b[33m";

    fn extract_output(op: ToolOperation) -> String {
        let env = fixture_environment();
        match op.to_content(&env) {
            Some(ChatResponseContent::ToolOutput(text)) => text,
            other => panic!("Expected ToolOutput, got: {other:?}"),
        }
    }

    /// ADD_TASK → PENDING: new task should be bold white.
    #[test]
    fn test_todo_lifecycle_add_pending() {
        use forge_domain::{Todo, TodoStatus};

        let after = vec![Todo::new("Buy milk").id("1").status(TodoStatus::Pending)];
        let actual = extract_output(ToolOperation::TodoWrite { before: vec![], after });

        assert!(
            actual.contains(BOLD_WHITE),
            "New pending task should be bold white, got: {actual:?}"
        );
    }

    /// ADD_TASK → WIP: new task created directly as in-progress should be bold
    /// cyan.
    #[test]
    fn test_todo_lifecycle_add_wip() {
        use forge_domain::{Todo, TodoStatus};

        let after = vec![Todo::new("Buy milk").id("1").status(TodoStatus::InProgress)];
        let actual = extract_output(ToolOperation::TodoWrite { before: vec![], after });

        assert!(
            actual.contains(BOLD_CYAN),
            "New in-progress task should be bold cyan, got: {actual:?}"
        );
    }

    /// ADD_TASK → DONE: new task created directly as completed should be bold
    /// green.
    #[test]
    fn test_todo_lifecycle_add_done() {
        use forge_domain::{Todo, TodoStatus};

        let after = vec![Todo::new("Buy milk").id("1").status(TodoStatus::Completed)];
        let actual = extract_output(ToolOperation::TodoWrite { before: vec![], after });

        assert!(
            actual.contains(BOLD_GREEN),
            "New completed task should be bold green, got: {actual:?}"
        );
    }

    /// PENDING → WIP: state change should be bold cyan.
    #[test]
    fn test_todo_lifecycle_pending_to_wip() {
        use forge_domain::{Todo, TodoStatus};

        let before = vec![Todo::new("Buy milk").id("1").status(TodoStatus::Pending)];
        let after = vec![Todo::new("Buy milk").id("1").status(TodoStatus::InProgress)];
        let actual = extract_output(ToolOperation::TodoWrite { before, after });

        assert!(
            actual.contains(BOLD_CYAN),
            "Pending→WIP task should be bold cyan, got: {actual:?}"
        );
    }

    /// WIP → DONE: state change should be bold green.
    #[test]
    fn test_todo_lifecycle_wip_to_done() {
        use forge_domain::{Todo, TodoStatus};

        let before = vec![Todo::new("Buy milk").id("1").status(TodoStatus::InProgress)];
        let after = vec![Todo::new("Buy milk").id("1").status(TodoStatus::Completed)];
        let actual = extract_output(ToolOperation::TodoWrite { before, after });

        assert!(
            actual.contains(BOLD_GREEN),
            "WIP→Done task should be bold green, got: {actual:?}"
        );
    }

    /// Full lifecycle: PENDING unchanged, WIP unchanged, DONE unchanged — all
    /// dim with state colors.
    #[test]
    fn test_todo_lifecycle_unchanged_all_dim() {
        use forge_domain::{Todo, TodoStatus};

        let todos = vec![
            Todo::new("Task A").id("1").status(TodoStatus::Pending),
            Todo::new("Task B").id("2").status(TodoStatus::InProgress),
            Todo::new("Task C").id("3").status(TodoStatus::Completed),
        ];
        let actual =
            extract_output(ToolOperation::TodoWrite { before: todos.clone(), after: todos });

        assert!(
            actual.contains(DIM_WHITE),
            "Unchanged pending task should be dim white, got: {actual:?}"
        );
        assert!(
            actual.contains(DIM_CYAN),
            "Unchanged in-progress task should be dim cyan, got: {actual:?}"
        );
        assert!(
            actual.contains(DIM_GREEN),
            "Unchanged completed task should be dim green, got: {actual:?}"
        );
        assert!(
            !actual.contains(BOLD_WHITE)
                && !actual.contains(BOLD_CYAN)
                && !actual.contains(BOLD_GREEN),
            "Unchanged tasks should not be bold, got: {actual:?}"
        );
    }

    /// Removed task should be yellow with strikethrough.
    #[test]
    fn test_todo_lifecycle_removed_yellow() {
        use forge_domain::{Todo, TodoStatus};

        let before = vec![Todo::new("Buy milk").id("1").status(TodoStatus::Pending)];
        let actual = extract_output(ToolOperation::TodoWrite { before, after: vec![] });

        assert!(
            actual.contains(YELLOW),
            "Removed task should be yellow, got: {actual:?}"
        );
        // Strikethrough escape: \x1b[9m
        assert!(
            actual.contains("\x1b[9m"),
            "Removed task content should have strikethrough, got: {actual:?}"
        );
    }

    /// Mixed: one unchanged (dim), one changed state (bold), one new (bold).
    #[test]
    fn test_todo_lifecycle_mixed_diff() {
        use forge_domain::{Todo, TodoStatus};

        let before = vec![
            Todo::new("Task A").id("1").status(TodoStatus::Pending), // unchanged
            Todo::new("Task B").id("2").status(TodoStatus::Pending), // pending → wip
        ];
        let after = vec![
            Todo::new("Task A").id("1").status(TodoStatus::Pending), // unchanged → dim white
            Todo::new("Task B").id("2").status(TodoStatus::InProgress), // changed → bold cyan
            Todo::new("Task C").id("3").status(TodoStatus::Pending), // new → bold white
        ];
        let actual = extract_output(ToolOperation::TodoWrite { before, after });

        assert!(
            actual.contains(DIM_WHITE),
            "Unchanged pending task should be dim white, got: {actual:?}"
        );
        assert!(
            actual.contains(BOLD_CYAN),
            "Changed task (now WIP) should be bold cyan, got: {actual:?}"
        );
        assert!(
            actual.contains(BOLD_WHITE),
            "New pending task should be bold white, got: {actual:?}"
        );
    }

    #[test]
    fn test_todo_write_realistic() {
        use forge_domain::{Todo, TodoStatus};

        // Marking task 1 as completed, adding new task 2
        let before = vec![
            Todo::new("Implement user authentication")
                .id("1")
                .status(TodoStatus::InProgress),
        ];
        let after = vec![
            Todo::new("Implement user authentication")
                .id("1")
                .status(TodoStatus::Completed),
            Todo::new("Walk the dog")
                .id("2")
                .status(TodoStatus::Pending),
        ];

        let fixture = ToolOperation::TodoWrite { before, after };
        let env = fixture_environment();

        let actual = fixture.to_content(&env);
        assert!(actual.is_some());
        if let Some(ChatResponseContent::ToolOutput(text)) = actual {
            let plain = strip_ansi_codes(text.as_str());
            assert!(plain.contains("Implement user authentication"));
            assert!(plain.contains("Walk the dog"));
        } else {
            panic!("Expected ToolOutput content");
        }
    }
}
