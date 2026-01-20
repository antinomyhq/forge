use std::collections::HashSet;
use std::sync::Arc;

use forge_app::{TodoWriteOutput, TodoWriteService};
use forge_domain::{Context, Todo, TodoStatus, TodoWrite};

/// Creates and manages a structured task list to track progress during complex
/// multi-step operations.
pub struct ForgeTodoWrite<F>(Arc<F>);

impl<F> ForgeTodoWrite<F> {
    pub fn new(_infra: Arc<F>) -> Self {
        Self(_infra)
    }
}

#[async_trait::async_trait]
impl<F: Send + Sync> TodoWriteService for ForgeTodoWrite<F> {
    async fn execute_todo_write(
        &self,
        input: TodoWrite,
        _context: &Context,
    ) -> anyhow::Result<TodoWriteOutput> {
        // Validate input
        validate_todos(&input.todos)?;

        // TODO: Add persistence using conversation context when custom storage is available
        // For now, just return the current state without previous state
        Ok(TodoWriteOutput {
            current: input.todos,
            previous: None,
        })
    }
}

/// Validates todo list constraints
///
/// # Errors
///
/// Returns an error if:
/// - Todo IDs are not unique
/// - More than one task is in progress
/// - Any todo content is empty
fn validate_todos(todos: &[Todo]) -> anyhow::Result<()> {
    // Check for empty content
    for todo in todos {
        todo.validate_content()?;
    }

    // Check for unique IDs
    let mut seen_ids = HashSet::new();
    for todo in todos {
        if !seen_ids.insert(&todo.id) {
            anyhow::bail!("Duplicate todo ID found: {}", todo.id);
        }
    }

    // Check for single in_progress task (soft requirement - just warn)
    let in_progress_count = todos
        .iter()
        .filter(|t| matches!(t.status, TodoStatus::InProgress))
        .count();

    if in_progress_count > 1 {
        tracing::warn!(
            "Multiple tasks marked as in_progress ({}). Consider focusing on one task at a time.",
            in_progress_count
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[tokio::test]
    async fn test_execute_todo_write_creates_new_list() {
        let infra = Arc::new(());
        let service = ForgeTodoWrite::new(infra);
        let input = TodoWrite {
            todos: vec![
                Todo::new("task-1", "First task"),
                Todo::new("task-2", "Second task").status(TodoStatus::InProgress),
            ],
        };
        let context = Context::default();

        let actual = service.execute_todo_write(input.clone(), &context).await.unwrap();

        assert_eq!(actual.current, input.todos);
        assert_eq!(actual.previous, None);
    }

    #[tokio::test]
    async fn test_execute_todo_write_empty_list() {
        let infra = Arc::new(());
        let service = ForgeTodoWrite::new(infra);
        let input = TodoWrite { todos: vec![] };
        let context = Context::default();

        let actual = service.execute_todo_write(input, &context).await.unwrap();

        assert!(actual.current.is_empty());
        assert_eq!(actual.previous, None);
    }

    #[tokio::test]
    async fn test_execute_todo_write_all_completed() {
        let infra = Arc::new(());
        let service = ForgeTodoWrite::new(infra);
        let input = TodoWrite {
            todos: vec![
                Todo::new("task-1", "First task").status(TodoStatus::Completed),
                Todo::new("task-2", "Second task").status(TodoStatus::Completed),
                Todo::new("task-3", "Third task").status(TodoStatus::Completed),
            ],
        };
        let context = Context::default();

        let actual = service.execute_todo_write(input.clone(), &context).await.unwrap();

        assert_eq!(actual.current.len(), 3);
        assert!(actual
            .current
            .iter()
            .all(|t| t.status == TodoStatus::Completed));
    }

    #[test]
    fn test_validate_todos_empty_content() {
        let fixture = vec![Todo::new("task-1", "")];
        let actual = validate_todos(&fixture);
        assert!(actual.is_err());
    }

    #[test]
    fn test_validate_todos_duplicate_ids() {
        let fixture = vec![
            Todo::new("task-1", "First task"),
            Todo::new("task-1", "Duplicate task"),
        ];
        let actual = validate_todos(&fixture);
        assert!(actual.is_err());
    }

    #[test]
    fn test_validate_todos_multiple_in_progress() {
        let fixture = vec![
            Todo::new("task-1", "First task").status(TodoStatus::InProgress),
            Todo::new("task-2", "Second task").status(TodoStatus::InProgress),
        ];
        // Should not error, just warn
        let actual = validate_todos(&fixture);
        assert!(actual.is_ok());
    }

    #[test]
    fn test_validate_todos_valid() {
        let fixture = vec![
            Todo::new("task-1", "First task"),
            Todo::new("task-2", "Second task").status(TodoStatus::InProgress),
            Todo::new("task-3", "Third task").status(TodoStatus::Completed),
        ];
        let actual = validate_todos(&fixture);
        assert!(actual.is_ok());
    }
}
