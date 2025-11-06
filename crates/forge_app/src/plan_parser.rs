use std::path::PathBuf;

use forge_domain::{ActivePlan, Task, TaskStatus};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

/// Parse a plan file and create an ActivePlan
///
/// Parses markdown task lists with the following syntax:
/// - `- [ ] Task description` for pending tasks (standard markdown)
/// - `- [x] Task description` for completed tasks (standard markdown)
/// - `- [~] Task description` for in-progress tasks (custom)
/// - `- [!] Task description` for failed tasks (custom)
pub fn parse_plan(path: PathBuf, content: &str) -> ActivePlan {
    ActivePlan::new(path, parse_tasks(content))
}

struct TaskParser {
    tasks: Vec<Task>,
    current_item: Option<ListItem>,
}

struct ListItem {
    text: String,
    status: Option<TaskStatus>,
    line_number: usize,
}

impl TaskParser {
    fn new() -> Self {
        Self { tasks: Vec::new(), current_item: None }
    }

    fn start_item(&mut self, line_number: usize) {
        self.current_item = Some(ListItem { text: String::new(), status: None, line_number });
    }

    fn set_checkbox_status(&mut self, checked: bool) {
        if let Some(item) = &mut self.current_item {
            item.status = Some(if checked {
                TaskStatus::Done
            } else {
                TaskStatus::Pending
            });
        }
    }

    fn append_text(&mut self, text: &str) {
        if let Some(item) = &mut self.current_item {
            item.text.push_str(text);
        }
    }

    fn append_space(&mut self) {
        if let Some(item) = &mut self.current_item {
            item.text.push(' ');
        }
    }

    fn end_item(&mut self) {
        if let Some(item) = self.current_item.take()
            && let Some(task) = Self::parse_list_item(item)
        {
            self.tasks.push(task);
        }
    }

    fn parse_list_item(item: ListItem) -> Option<Task> {
        let description = item.text.trim();
        if description.is_empty() {
            return None;
        }

        let (status, description) = match item.status {
            Some(status) => (status, description),
            None => Self::parse_custom_marker(description)?,
        };

        let description = description.trim();
        (!description.is_empty())
            .then(|| Task::new(description.to_string(), status, item.line_number))
    }

    fn parse_custom_marker(text: &str) -> Option<(TaskStatus, &str)> {
        if let Some(desc) = text.strip_prefix("[~]") {
            Some((TaskStatus::InProgress, desc))
        } else if let Some(desc) = text.strip_prefix("[!]") {
            Some((TaskStatus::Failed, desc))
        } else {
            None
        }
    }

    fn into_tasks(self) -> Vec<Task> {
        self.tasks
    }
}

fn parse_tasks(content: &str) -> Vec<Task> {
    let options = Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(content, options);
    let mut task_parser = TaskParser::new();

    for (event, range) in parser.into_offset_iter() {
        let line_number = content[..range.start].lines().count() + 1;

        match event {
            Event::Start(Tag::Item) => task_parser.start_item(line_number),
            Event::TaskListMarker(checked) => task_parser.set_checkbox_status(checked),
            Event::End(TagEnd::Item) => task_parser.end_item(),
            Event::Text(text) | Event::Code(text) => task_parser.append_text(&text),
            Event::SoftBreak | Event::HardBreak => task_parser.append_space(),
            _ => {}
        }
    }

    task_parser.into_tasks()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn fixture_path() -> PathBuf {
        PathBuf::from("/test/plan.md")
    }

    #[test]
    fn test_parse_plan_with_all_statuses() {
        let content = r#"# Test Plan

Some introduction text.

## Tasks

- [ ] PENDING - Task 1
- [~] IN_PROGRESS - Task 2
- [x] DONE - Task 3
- [!] FAILED - Task 4
"#;
        let plan = parse_plan(fixture_path(), content);

        assert_eq!(plan.total(), 4);
        assert_eq!(plan.todo(), 1);
        assert_eq!(plan.in_progress(), 1);
        assert_eq!(plan.completed(), 1);
        assert_eq!(plan.failed(), 1);
    }

    #[test]
    fn test_parse_plan_empty() {
        let plan = parse_plan(fixture_path(), "# Plan\n\nNo tasks");

        assert_eq!(plan.total(), 0);
        assert_eq!(plan.todo(), 0);
        assert_eq!(plan.in_progress(), 0);
        assert_eq!(plan.completed(), 0);
        assert_eq!(plan.failed(), 0);
    }

    #[test]
    fn test_parse_plan_all_completed() {
        let content = r#"
## Tasks

- [x] Task 1
- [x] Task 2
- [x] Task 3
"#;
        let plan = parse_plan(fixture_path(), content);

        assert_eq!(plan.completed(), 3);
        assert_eq!(plan.todo(), 0);
        assert!(plan.is_complete());
    }

    #[test]
    fn test_parse_plan_with_asterisk_lists() {
        let content = r#"
* [ ] Task with asterisk
* [x] Completed task with asterisk
"#;
        let plan = parse_plan(fixture_path(), content);

        assert_eq!(plan.total(), 2);
        assert_eq!(plan.todo(), 1);
        assert_eq!(plan.completed(), 1);
    }

    #[test]
    fn test_parse_plan_ignores_non_task_lines() {
        let content = r#"
# Header
Some text
- Regular list item without checkbox
- [x] Task 1
Regular paragraph
- [ ] Task 2
"#;
        let plan = parse_plan(fixture_path(), content);

        assert_eq!(plan.total(), 2);
        assert_eq!(plan.completed(), 1);
        assert_eq!(plan.todo(), 1);
    }

    #[test]
    fn test_parse_plan_with_uppercase_x() {
        let plan = parse_plan(fixture_path(), "- [X] Task with uppercase X");

        assert_eq!(plan.total(), 1);
        assert_eq!(plan.completed(), 1);
    }

    #[test]
    fn test_parse_plan_mixed_standard_and_custom() {
        let content = r#"
- [ ] Standard pending
- [~] Custom in-progress
- [x] Standard done
- [!] Custom failed
"#;
        let plan = parse_plan(fixture_path(), content);

        assert_eq!(plan.total(), 4);
        assert_eq!(plan.todo(), 1);
        assert_eq!(plan.in_progress(), 1);
        assert_eq!(plan.completed(), 1);
        assert_eq!(plan.failed(), 1);
    }

    #[test]
    fn test_parse_plan_empty_task_descriptions_ignored() {
        let content = r#"
- [ ]
- [~]   
- [x] Valid task
"#;
        let plan = parse_plan(fixture_path(), content);

        assert_eq!(plan.total(), 1);
        assert_eq!(plan.completed(), 1);
    }
}
