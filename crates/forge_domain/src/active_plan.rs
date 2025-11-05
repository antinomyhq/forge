use std::path::PathBuf;

use serde::Serialize;

/// Represents a task status in a plan
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TaskStatus {
    /// Task is pending (not started)
    Pending,
    /// Task is currently in progress
    InProgress,
    /// Task is completed
    Done,
    /// Task has failed
    Failed,
}

impl TaskStatus {
    /// Parse task status from markdown checkbox syntax
    ///
    /// # Examples
    /// - `[ ]` or `[]` -> Pending (empty checkbox, with or without space)
    /// - `[~]` -> InProgress
    /// - `[x]` or `[X]` -> Done
    /// - `[!]` -> Failed
    pub fn from_checkbox(checkbox: &str) -> Option<Self> {
        match checkbox.trim() {
            // Accept both "[]" and "[ ]" as pending to handle whitespace variations
            "[ ]" | "[]" => Some(TaskStatus::Pending),
            "[~]" => Some(TaskStatus::InProgress),
            "[x]" | "[X]" => Some(TaskStatus::Done),
            "[!]" => Some(TaskStatus::Failed),
            _ => None,
        }
    }

    /// Convert task status to markdown checkbox syntax
    pub fn to_checkbox(&self) -> &'static str {
        match self {
            TaskStatus::Pending => "[ ]",
            TaskStatus::InProgress => "[~]",
            TaskStatus::Done => "[x]",
            TaskStatus::Failed => "[!]",
        }
    }
}

/// Represents a single task in a plan
#[derive(Debug, Clone, Serialize)]
pub struct Task {
    /// The task description/title
    pub description: String,
    /// The current status of the task
    pub status: TaskStatus,
    /// The line number where this task appears in the plan file
    pub line_number: usize,
}

impl Task {
    /// Creates a new task
    pub fn new(description: String, status: TaskStatus, line_number: usize) -> Self {
        Self { description, status, line_number }
    }

    /// Check if the task is complete
    pub fn is_complete(&self) -> bool {
        self.status == TaskStatus::Done
    }

    /// Check if the task is pending
    pub fn is_pending(&self) -> bool {
        self.status == TaskStatus::Pending
    }

    /// Check if the task is in progress
    pub fn is_in_progress(&self) -> bool {
        self.status == TaskStatus::InProgress
    }

    /// Check if the task has failed
    pub fn has_failed(&self) -> bool {
        self.status == TaskStatus::Failed
    }
}

/// Represents an active plan with tasks
#[derive(Debug, Clone, Serialize)]
pub struct ActivePlan {
    pub path: PathBuf,
    pub tasks: Vec<Task>,
}

impl ActivePlan {
    /// Creates a new ActivePlan with tasks
    pub fn new(path: PathBuf, tasks: Vec<Task>) -> Self {
        Self { path, tasks }
    }

    /// Check if the plan is complete (all tasks are done and no tasks are
    /// pending or in progress)
    pub fn is_complete(&self) -> bool {
        !self.tasks.is_empty()
            && self.tasks.iter().all(|t| t.is_complete())
            && !self
                .tasks
                .iter()
                .any(|t| t.is_pending() || t.is_in_progress() || t.has_failed())
    }

    /// Get completion percentage
    pub fn complete_percentage(&self) -> f32 {
        if self.tasks.is_empty() {
            return 0.0;
        }
        let completed = self.tasks.iter().filter(|t| t.is_complete()).count();
        completed as f32 / self.tasks.len() as f32
    }

    /// Get the total number of tasks
    pub fn total(&self) -> usize {
        self.tasks.len()
    }

    /// Get the number of completed tasks
    pub fn completed(&self) -> usize {
        self.tasks.iter().filter(|t| t.is_complete()).count()
    }

    /// Get the number of pending tasks
    pub fn todo(&self) -> usize {
        self.tasks.iter().filter(|t| t.is_pending()).count()
    }

    /// Get the number of in-progress tasks
    pub fn in_progress(&self) -> usize {
        self.tasks.iter().filter(|t| t.is_in_progress()).count()
    }

    /// Get the number of failed tasks
    pub fn failed(&self) -> usize {
        self.tasks.iter().filter(|t| t.has_failed()).count()
    }

    /// Get the next pending task
    pub fn next_pending_task(&self) -> Option<&Task> {
        self.tasks.iter().find(|t| t.is_pending())
    }

    /// Get all tasks with a specific status
    pub fn tasks_with_status(&self, status: TaskStatus) -> Vec<&Task> {
        self.tasks.iter().filter(|t| t.status == status).collect()
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_task_status_from_checkbox() {
        assert_eq!(TaskStatus::from_checkbox("[ ]"), Some(TaskStatus::Pending));
        assert_eq!(
            TaskStatus::from_checkbox("[~]"),
            Some(TaskStatus::InProgress)
        );
        assert_eq!(TaskStatus::from_checkbox("[x]"), Some(TaskStatus::Done));
        assert_eq!(TaskStatus::from_checkbox("[X]"), Some(TaskStatus::Done));
        assert_eq!(TaskStatus::from_checkbox("[!]"), Some(TaskStatus::Failed));
        assert_eq!(TaskStatus::from_checkbox("[?]"), None);
    }

    #[test]
    fn test_task_status_to_checkbox() {
        assert_eq!(TaskStatus::Pending.to_checkbox(), "[ ]");
        assert_eq!(TaskStatus::InProgress.to_checkbox(), "[~]");
        assert_eq!(TaskStatus::Done.to_checkbox(), "[x]");
        assert_eq!(TaskStatus::Failed.to_checkbox(), "[!]");
    }

    #[test]
    fn test_task_creation() {
        let task = Task::new("Test task".to_string(), TaskStatus::Pending, 1);
        assert_eq!(task.description, "Test task");
        assert_eq!(task.status, TaskStatus::Pending);
        assert_eq!(task.line_number, 1);
    }

    #[test]
    fn test_task_status_checks() {
        let pending = Task::new("Pending".to_string(), TaskStatus::Pending, 1);
        assert!(pending.is_pending());
        assert!(!pending.is_complete());

        let done = Task::new("Done".to_string(), TaskStatus::Done, 2);
        assert!(done.is_complete());
        assert!(!done.is_pending());

        let in_progress = Task::new("In Progress".to_string(), TaskStatus::InProgress, 3);
        assert!(in_progress.is_in_progress());
        assert!(!in_progress.is_complete());

        let failed = Task::new("Failed".to_string(), TaskStatus::Failed, 4);
        assert!(failed.has_failed());
        assert!(!failed.is_complete());
    }

    #[test]
    fn test_active_plan_creation() {
        let tasks = vec![
            Task::new("Task 1".to_string(), TaskStatus::Pending, 1),
            Task::new("Task 2".to_string(), TaskStatus::Done, 2),
        ];
        let plan = ActivePlan::new(PathBuf::from("/test/plan.md"), tasks);

        assert_eq!(plan.total(), 2);
        assert_eq!(plan.completed(), 1);
        assert_eq!(plan.todo(), 1);
    }

    #[test]
    fn test_active_plan_empty() {
        let plan = ActivePlan::new(PathBuf::from("/test/plan.md"), vec![]);

        assert_eq!(plan.total(), 0);
        assert_eq!(plan.completed(), 0);
        assert_eq!(plan.todo(), 0);
        assert_eq!(plan.in_progress(), 0);
        assert_eq!(plan.failed(), 0);
    }

    #[test]
    fn test_active_plan_all_completed() {
        let tasks = vec![
            Task::new("Task 1".to_string(), TaskStatus::Done, 1),
            Task::new("Task 2".to_string(), TaskStatus::Done, 2),
            Task::new("Task 3".to_string(), TaskStatus::Done, 3),
        ];
        let plan = ActivePlan::new(PathBuf::from("/test/plan.md"), tasks);

        assert_eq!(plan.completed(), 3);
        assert_eq!(plan.todo(), 0);
        assert!(plan.is_complete());
    }

    #[test]
    fn test_active_plan_methods() {
        let tasks = vec![
            Task::new("Task 1".to_string(), TaskStatus::Done, 1),
            Task::new("Task 2".to_string(), TaskStatus::Pending, 2),
            Task::new("Task 3".to_string(), TaskStatus::InProgress, 3),
        ];
        let plan = ActivePlan::new(PathBuf::from("/test/plan.md"), tasks);

        assert_eq!(plan.complete_percentage(), 1.0 / 3.0);
        assert!(!plan.is_complete());

        let next = plan.next_pending_task().unwrap();
        assert_eq!(next.description, "Task 2");

        let pending_tasks = plan.tasks_with_status(TaskStatus::Pending);
        assert_eq!(pending_tasks.len(), 1);
    }

    #[test]
    fn test_active_plan_with_all_statuses() {
        let tasks = vec![
            Task::new("Task 1".to_string(), TaskStatus::Pending, 1),
            Task::new("Task 2".to_string(), TaskStatus::InProgress, 2),
            Task::new("Task 3".to_string(), TaskStatus::Done, 3),
            Task::new("Task 4".to_string(), TaskStatus::Failed, 4),
        ];
        let plan = ActivePlan::new(PathBuf::from("/test/plan.md"), tasks);

        assert_eq!(plan.total(), 4);
        assert_eq!(plan.todo(), 1);
        assert_eq!(plan.in_progress(), 1);
        assert_eq!(plan.completed(), 1);
        assert_eq!(plan.failed(), 1);
    }
}
