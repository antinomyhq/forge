use forge_display::TitleFormat;
use forge_domain::Task;

pub fn format_tasks(tasks: &[Task]) -> String {
    if tasks.is_empty() {
        return TitleFormat::action("No tasks found".to_string()).to_string();
    }

    let mut output = TitleFormat::action(format!("Found {} task(s)", tasks.len())).to_string();
    output.push('\n');

    for (index, task) in tasks.iter().enumerate() {
        output.push_str(&format!(
            "\n{}. {} [{}]",
            index + 1,
            task.task,
            task.status.status_name()
        ));
    }

    output
}

#[cfg(test)]
mod tests {
    use forge_domain::Status;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_format_tasks_empty() {
        let fixture = vec![];
        let actual = format_tasks(&fixture);
        let expected = TitleFormat::action("No tasks found".to_string()).to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_tasks_single() {
        let fixture = vec![Task::new(1, "Complete the feature").status(Status::InProgress)];
        let actual = format_tasks(&fixture);
        let expected = format!(
            "{}\n\n1. Complete the feature [IN_PROGRESS]",
            TitleFormat::action("Found 1 task(s)".to_string())
        );
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_tasks_multiple() {
        let fixture = vec![
            Task::new(1, "Complete the feature").status(Status::InProgress),
            Task::new(2, "Write tests").status(Status::Pending),
            Task::new(3, "Deploy to production").status(Status::Done),
        ];
        let actual = format_tasks(&fixture);
        let expected = format!(
            "{}\n\n1. Complete the feature [IN_PROGRESS]\n2. Write tests [PENDING]\n3. Deploy to production [DONE]",
            TitleFormat::action("Found 3 task(s)".to_string())
        );
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_tasks_minimal() {
        let fixture = vec![Task::new(1, "Simple task")];
        let actual = format_tasks(&fixture);
        let expected = format!(
            "{}\n\n1. Simple task [PENDING]",
            TitleFormat::action("Found 1 task(s)".to_string())
        );
        assert_eq!(actual, expected);
    }
}
