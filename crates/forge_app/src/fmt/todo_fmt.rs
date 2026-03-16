use forge_domain::{Todo, TodoStatus};

/// Controls the styling applied to a rendered todo line.
enum TodoLineStyle {
    /// Bold styling used for new or changed todos.
    Bold,
    /// Dim styling used for unchanged todos.
    Dim,
}

/// Renders one todo line with icon and ANSI styling.
///
/// # Arguments
///
/// * `todo` - Todo item to render.
/// * `line_style` - Emphasis style for the line.
fn format_todo_line(todo: &Todo, line_style: TodoLineStyle) -> String {
    use console::style;

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

/// Formats a todo diff showing all todos in `after` plus removed todos from
/// `before`.
///
/// # Arguments
///
/// * `before` - Previous todo list state.
/// * `after` - New todo list state.
pub(crate) fn format_todos_diff(before: &[Todo], after: &[Todo]) -> String {
    use console::style;

    let before_map: std::collections::HashMap<&str, &Todo> =
        before.iter().map(|todo| (todo.id.as_str(), todo)).collect();
    let after_ids: std::collections::HashSet<&str> =
        after.iter().map(|todo| todo.id.as_str()).collect();

    let mut result = "\n".to_string();

    enum DiffLine<'a> {
        Current {
            todo: &'a Todo,
            line_style: TodoLineStyle,
        },
        Removed {
            todo: &'a Todo,
        },
    }

    impl DiffLine<'_> {
        fn id(&self) -> &str {
            match self {
                DiffLine::Current { todo, .. } | DiffLine::Removed { todo } => todo.id.as_str(),
            }
        }
    }

    let mut lines: Vec<DiffLine<'_>> = Vec::new();

    for todo in after {
        let previous = before_map.get(todo.id.as_str()).copied();
        let is_new = previous.is_none();
        let is_changed = previous
            .map(|item| item.status != todo.status || item.content != todo.content)
            .unwrap_or(false);

        let line_style = if is_new || is_changed {
            TodoLineStyle::Bold
        } else {
            TodoLineStyle::Dim
        };

        lines.push(DiffLine::Current { todo, line_style });
    }

    for todo in before {
        if !after_ids.contains(todo.id.as_str()) {
            lines.push(DiffLine::Removed { todo });
        }
    }

    lines.sort_by(|left, right| left.id().cmp(right.id()));

    for line in lines {
        match line {
            DiffLine::Current { todo, line_style } => {
                result.push_str(&format_todo_line(todo, line_style));
            }
            DiffLine::Removed { todo } => {
                let content = style(todo.content.as_str()).strikethrough().to_string();

                if todo.status == TodoStatus::Completed {
                    result.push_str(&format!(
                        "  {}\n",
                        style(format!("󰄵 {content}")).white().dim()
                    ));
                } else {
                    result.push_str(&format!("  {}\n", style(format!("󰄱 {content}")).red()));
                }
            }
        }
    }

    result
}

/// Formats todos as ANSI-styled checklist lines.
///
/// # Arguments
///
/// * `todos` - Todo list to format.
pub(crate) fn format_todos(todos: &[Todo]) -> String {
    if todos.is_empty() {
        return String::new();
    }

    let mut result = "\n".to_string();

    let mut sorted_todos: Vec<&Todo> = todos.iter().collect();
    sorted_todos.sort_by(|left, right| left.id.cmp(&right.id));

    for todo in sorted_todos {
        result.push_str(&format_todo_line(todo, TodoLineStyle::Dim));
    }

    result
}

#[cfg(test)]
mod tests {
    use console::strip_ansi_codes;
    use forge_domain::{ChatResponseContent, Environment, Todo, TodoStatus};
    use insta::assert_snapshot;
    use pretty_assertions::assert_eq;

    use crate::fmt::content::FormatContent;
    use crate::operation::ToolOperation;

    fn fixture_environment() -> Environment {
        use fake::{Fake, Faker};

        let max_bytes: f64 = 250.0 * 1024.0;
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

    fn fixture_todo(content: &str, id: &str, status: TodoStatus) -> Todo {
        Todo::new(content).id(id).status(status)
    }

    fn fixture_todo_write_output(before: Vec<Todo>, after: Vec<Todo>) -> String {
        let setup = ToolOperation::TodoWrite { before, after };
        let actual = setup.to_content(&fixture_environment());

        if let Some(ChatResponseContent::ToolOutput(output)) = actual {
            strip_ansi_codes(output.as_str()).to_string()
        } else {
            panic!("Expected ToolOutput content")
        }
    }

    #[test]
    fn test_todo_write_mixed_changes_snapshot() {
        let setup = (
            vec![
                fixture_todo("Task 1", "1", TodoStatus::Pending),
                fixture_todo("Task 2", "2", TodoStatus::InProgress),
            ],
            vec![
                fixture_todo("Task 1", "1", TodoStatus::Completed),
                fixture_todo("Task 3", "3", TodoStatus::Pending),
            ],
        );

        let actual = fixture_todo_write_output(setup.0, setup.1);
        assert_snapshot!(actual);
    }

    #[test]
    fn test_todo_write_removed_completed_todos_render_as_dimmed_done() {
        let setup = (
            vec![fixture_todo("Done", "1", TodoStatus::Completed)],
            Vec::new(),
        );

        let actual = fixture_todo_write_output(setup.0, setup.1);
        let expected = "\n  󰄵 Done\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_todos_are_sorted_by_id() {
        let setup = vec![
            fixture_todo("Second", "2", TodoStatus::Pending),
            fixture_todo("First", "1", TodoStatus::Pending),
        ];

        let actual = strip_ansi_codes(super::format_todos(&setup).as_str()).to_string();
        let expected = "\n  󰄱 First\n  󰄱 Second\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_todo_write_dump_flow_in_same_order() {
        let step_1 = vec![
            fixture_todo(
                "Generate JSONL input file with all 59 cases",
                "1",
                TodoStatus::InProgress,
            ),
            fixture_todo(
                "Create JSON schema file for structured output",
                "2",
                TodoStatus::Pending,
            ),
            fixture_todo("Create system prompt template", "3", TodoStatus::Pending),
            fixture_todo("Create user prompt template", "4", TodoStatus::Pending),
            fixture_todo("Test with 2-3 cases first", "5", TodoStatus::Pending),
            fixture_todo("Run for all cases", "6", TodoStatus::Pending),
        ];
        let step_2 = vec![
            fixture_todo(
                "Generate JSONL input file with all 59 cases",
                "1",
                TodoStatus::Completed,
            ),
            fixture_todo(
                "Create JSON schema file for structured output",
                "2",
                TodoStatus::InProgress,
            ),
            fixture_todo("Create system prompt template", "3", TodoStatus::Pending),
            fixture_todo("Create user prompt template", "4", TodoStatus::Pending),
            fixture_todo("Test with 2-3 cases first", "5", TodoStatus::Pending),
            fixture_todo("Run for all cases", "6", TodoStatus::Pending),
        ];
        let step_3 = vec![
            fixture_todo(
                "Create JSON schema file for structured output",
                "2",
                TodoStatus::Completed,
            ),
            fixture_todo("Create system prompt template", "3", TodoStatus::InProgress),
            fixture_todo("Create user prompt template", "4", TodoStatus::Pending),
            fixture_todo("Test with 2-3 cases first", "5", TodoStatus::Pending),
            fixture_todo("Run for all cases", "6", TodoStatus::Pending),
        ];
        let step_4 = vec![
            fixture_todo("Create system prompt template", "3", TodoStatus::Completed),
            fixture_todo("Create user prompt template", "4", TodoStatus::Completed),
            fixture_todo("Test with 2-3 cases first", "5", TodoStatus::InProgress),
            fixture_todo("Run for all cases", "6", TodoStatus::Pending),
        ];

        let actual_1 = fixture_todo_write_output(Vec::new(), step_1.clone());
        let expected_1 = "\n  󰄗 Generate JSONL input file with all 59 cases\n  󰄱 Create JSON schema file for structured output\n  󰄱 Create system prompt template\n  󰄱 Create user prompt template\n  󰄱 Test with 2-3 cases first\n  󰄱 Run for all cases\n";
        assert_eq!(actual_1, expected_1);

        let actual_2 = fixture_todo_write_output(step_1.clone(), step_2.clone());
        let expected_2 = "\n  󰄵 Generate JSONL input file with all 59 cases\n  󰄗 Create JSON schema file for structured output\n  󰄱 Create system prompt template\n  󰄱 Create user prompt template\n  󰄱 Test with 2-3 cases first\n  󰄱 Run for all cases\n";
        assert_eq!(actual_2, expected_2);

        let actual_3 = fixture_todo_write_output(step_2.clone(), step_3.clone());
        let expected_3 = "\n  󰄵 Generate JSONL input file with all 59 cases\n  󰄵 Create JSON schema file for structured output\n  󰄗 Create system prompt template\n  󰄱 Create user prompt template\n  󰄱 Test with 2-3 cases first\n  󰄱 Run for all cases\n";
        assert_eq!(actual_3, expected_3);

        let actual_4 = fixture_todo_write_output(step_3, step_4);
        let expected_4 = "\n  󰄵 Create JSON schema file for structured output\n  󰄵 Create system prompt template\n  󰄵 Create user prompt template\n  󰄗 Test with 2-3 cases first\n  󰄱 Run for all cases\n";
        assert_eq!(actual_4, expected_4);
    }
}
