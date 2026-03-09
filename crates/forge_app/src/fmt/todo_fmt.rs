use forge_domain::Todo;

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

        result.push_str(&format_todo_line(todo, line_style));
    }

    for todo in before {
        if !after_ids.contains(todo.id.as_str()) {
            let content = style(todo.content.as_str()).strikethrough().to_string();
            result.push_str(&format!("  {}\n", style(format!("󰄱 {content}")).red()));
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

    for todo in todos {
        result.push_str(&format_todo_line(todo, TodoLineStyle::Dim));
    }

    result
}

#[cfg(test)]
mod tests {
    use console::strip_ansi_codes;
    use forge_domain::{ChatResponseContent, Environment, Todo, TodoStatus};
    use insta::assert_snapshot;

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
}
