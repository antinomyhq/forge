use derive_setters::Setters;
use eserde::Deserialize;
use schemars::JsonSchema;
use serde::Serialize;

/// Status of a todo task
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    /// Task is pending and not yet started
    Pending,
    /// Task is currently in progress
    InProgress,
    /// Task is completed
    Completed,
}

impl Default for TodoStatus {
    fn default() -> Self {
        Self::Pending
    }
}

/// Represents a single todo item in a task list
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Setters)]
#[setters(strip_option, into)]
pub struct Todo {
    /// Unique identifier for the todo item
    pub id: String,
    
    /// Content describing the task to be done
    pub content: String,
    
    /// Current status of the task
    #[serde(default)]
    pub status: TodoStatus,
}

impl Todo {
    /// Creates a new Todo with the given id and content
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier for the todo item
    /// * `content` - Description of the task
    pub fn new(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            content: content.into(),
            status: TodoStatus::default(),
        }
    }

    /// Validates that the todo content is non-empty
    ///
    /// # Errors
    ///
    /// Returns an error if the content is empty or whitespace-only
    pub fn validate_content(&self) -> anyhow::Result<()> {
        if self.content.trim().is_empty() {
            anyhow::bail!("Todo content cannot be empty");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_todo_new() {
        let actual = Todo::new("task-1", "Write tests");
        let expected = Todo {
            id: "task-1".to_string(),
            content: "Write tests".to_string(),
            status: TodoStatus::Pending,
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_todo_with_setters() {
        let actual = Todo::new("task-1", "Write tests")
            .status(TodoStatus::InProgress);
        
        let expected = Todo {
            id: "task-1".to_string(),
            content: "Write tests".to_string(),
            status: TodoStatus::InProgress,
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_todo_validate_content_valid() {
        let todo = Todo::new("task-1", "Write tests");
        assert!(todo.validate_content().is_ok());
    }

    #[test]
    fn test_todo_validate_content_empty() {
        let todo = Todo::new("task-1", "");
        assert!(todo.validate_content().is_err());
    }

    #[test]
    fn test_todo_validate_content_whitespace() {
        let todo = Todo::new("task-1", "   ");
        assert!(todo.validate_content().is_err());
    }

    #[test]
    fn test_todo_status_default() {
        let status = TodoStatus::default();
        assert_eq!(status, TodoStatus::Pending);
    }

    #[test]
    fn test_todo_serialization() {
        let fixture = Todo::new("task-1", "Write tests")
            .status(TodoStatus::InProgress);
        
        let actual = serde_json::to_value(&fixture).unwrap();
        let expected = serde_json::json!({
            "id": "task-1",
            "content": "Write tests",
            "status": "in_progress"
        });
        
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_todo_deserialization() {
        let json = serde_json::json!({
            "id": "task-1",
            "content": "Write tests",
            "status": "completed"
        });
        
        let actual: Todo = serde_json::from_value(json).unwrap();
        let expected = Todo::new("task-1", "Write tests")
            .status(TodoStatus::Completed);
        
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_todo_schema_generation() {
        let schema = schemars::schema_for!(Todo);
        assert!(schema.schema.object.is_some());
    }
}
