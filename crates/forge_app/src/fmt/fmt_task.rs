use forge_domain::{Status, TaskList};

pub fn to_markdown(before: &TaskList, after: &TaskList) -> String {
    if after.tasks().is_empty() {
        return "No tasks in the list.".to_string();
    }

    let mut markdown: Vec<String> = vec!["\n".to_string()];

    // Create a map of before tasks for comparison
    let before_tasks: std::collections::HashMap<i32, &forge_domain::Task> =
        before.tasks().iter().map(|task| (task.id, task)).collect();

    for task in after.tasks().iter() {
        // Check if task has changed by comparing with before state
        let task_changed = match before_tasks.get(&task.id) {
            Some(before_task) => before_task.status != task.status || before_task.task != task.task,
            None => true, // New task
        };

        let glyph = match task.status {
            Status::Pending => "☐",
            Status::InProgress => "▣",
            Status::Done => "◼",
        };

        let mut text = task.task.clone();

        if task_changed {
            text = format!("**{text}**");
        }

        text = match task.status {
            Status::Pending => text,
            Status::InProgress => format!("__{text}__"),
            Status::Done => format!("~~{text}~~"),
        };

        text = format!("{glyph} {text}");

        markdown.push(text);
    }

    markdown.push("\n".to_owned());

    markdown.join("\n")
}

#[cfg(test)]
mod tests {
    use forge_domain::TaskList;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_to_markdown_empty_task_list() {
        let before = TaskList::new();
        let fixture = TaskList::new();
        let actual = to_markdown(&before, &fixture);
        let expected = "No tasks in the list.";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_to_markdown_single_pending_task() {
        let before = TaskList::new();
        let mut fixture = TaskList::new();
        fixture.append("Write documentation");
        let actual = to_markdown(&before, &fixture);
        assert!(actual.contains("☐ **Write documentation**")); // New task should be bold
    }

    #[test]
    fn test_to_markdown_mixed_status_tasks() {
        let mut before = TaskList::new();
        let _task1 = before.append("First task");
        let task2 = before.append("Second task");
        before.append("Third task");

        let mut fixture = before.clone();
        // Mark second task as done
        fixture.mark_done(task2.id);
        // Mark first task as in progress manually
        fixture.get_task_mut(0).unwrap().mark_in_progress();

        let actual = to_markdown(&before, &fixture);
        assert!(actual.contains("**First task**")); // changed to in progress
        assert!(actual.contains("**Second task**")); // changed to done
        assert!(actual.contains("☐ Third task")); // unchanged pending
    }

    // Snapshot tests for markdown output
    #[test]
    fn test_empty_task_list_markdown_snapshot() {
        let before = TaskList::new();
        let fixture = TaskList::new();
        let actual = to_markdown(&before, &fixture);
        insta::assert_snapshot!(actual);
    }

    #[test]
    fn test_task_comparison_changes_highlighted() {
        // Test that demonstrates the comparison functionality
        let mut before = TaskList::new();
        let _task1 = before.append("Unchanged task");
        let _task2 = before.append("Task to be modified");
        let task3 = before.append("Task to be completed");

        let mut after = before.clone();
        // Modify task 2's text
        after.get_task_mut(1).unwrap().task = "Modified task text".to_string();
        // Complete task 3
        after.mark_done(task3.id);

        let actual = to_markdown(&before, &after);

        // Unchanged task should not be bold
        assert!(actual.contains("☐ Unchanged task"));
        // Modified task should be bold
        assert!(actual.contains("☐ **Modified task text**"));
        // Completed task should be bold and use done glyph
        assert!(actual.contains("◼ ~~**Task to be completed**~~"));
    }

    #[test]
    fn test_single_pending_task_markdown_snapshot() {
        let before = TaskList::new();
        let mut fixture = TaskList::new();
        fixture.append("Write documentation");
        let actual = to_markdown(&before, &fixture);
        insta::assert_snapshot!(actual);
    }

    #[test]
    fn test_mixed_status_tasks_markdown_snapshot() {
        let mut before = TaskList::new();
        let _task1 = before.append("First task");
        let task2 = before.append("Second task");
        before.append("Third task");

        let mut fixture = before.clone();
        // Mark second task as done
        fixture.mark_done(task2.id);
        // Mark first task as in progress manually
        fixture.get_task_mut(0).unwrap().mark_in_progress();

        let actual = to_markdown(&before, &fixture);
        insta::assert_snapshot!(actual);
    }

    #[test]
    fn test_all_done_tasks_markdown_snapshot() {
        let mut before = TaskList::new();
        let task1 = before.append("Complete feature A");
        let task2 = before.append("Write tests");
        let task3 = before.append("Update documentation");

        let mut fixture = before.clone();
        fixture.mark_done(task1.id);
        fixture.mark_done(task2.id);
        fixture.mark_done(task3.id);

        let actual = to_markdown(&before, &fixture);
        insta::assert_snapshot!(actual);
    }

    #[test]
    fn test_complex_task_list_markdown_snapshot() {
        let mut before = TaskList::new();
        let _task1 = before.append("Review pull request #123");
        let _task2 = before.append("Fix bug in authentication");
        let task3 = before.append("Deploy to staging");
        let _task4 = before.append("Update API documentation");
        let _task5 = before.append("Refactor user service");

        let mut fixture = before.clone();
        // Mark one task as done
        fixture.mark_done(task3.id);
        // Mark two tasks as in progress manually (without removing them)
        fixture.get_task_mut(0).unwrap().mark_in_progress(); // "Review pull request #123"
        fixture.get_task_mut(4).unwrap().mark_in_progress(); // "Refactor user service"

        let actual = to_markdown(&before, &fixture);
        insta::assert_snapshot!(actual);
    }
}
