use std::collections::VecDeque;

use derive_setters::Setters;
use eserde::Deserialize;
use schemars::JsonSchema;
use serde::Serialize;

use crate::TaskInput;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, JsonSchema)]
pub enum Status {
    #[default]
    Pending,
    InProgress,
    Done,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[setters(into, strip_option)]
pub struct Task {
    /// A monotonously increasing ID
    pub id: i32,

    /// The task instructions to be followed
    pub task: String,

    /// Status of the task
    pub status: Status,
    /// Optional category to group related tasks
    pub category: Option<String>,

    /// Detailed description of changes needed, with examples and optional file
    /// references
    pub note: Option<String>,

    /// List of files to refer
    pub files: Vec<String>,
}

impl Task {
    pub fn new(id: i32, task: impl Into<String>) -> Self {
        Self {
            id,
            task: task.into(),
            status: Status::default(),
            category: None,
            note: None,
            files: Vec::new(),
        }
    }

    pub fn mark_in_progress(&mut self) -> &mut Self {
        self.status = Status::InProgress;
        self
    }

    pub fn mark_done(&mut self) -> &mut Self {
        self.status = Status::Done;
        self
    }

    pub fn is_pending(&self) -> bool {
        matches!(self.status, Status::Pending)
    }

    pub fn is_in_progress(&self) -> bool {
        matches!(self.status, Status::InProgress)
    }

    pub fn is_done(&self) -> bool {
        matches!(self.status, Status::Done)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskStats {
    pub total_tasks: u32,
    pub done_tasks: u32,
    pub pending_tasks: u32,
    pub in_progress_tasks: u32,
}

impl TaskStats {
    fn new(total_tasks: u32, done_tasks: u32, pending_tasks: u32, in_progress_tasks: u32) -> Self {
        Self { total_tasks, done_tasks, pending_tasks, in_progress_tasks }
    }

    pub fn from_tasks(tasks: &VecDeque<Task>) -> Self {
        let total_tasks = tasks.len() as u32;
        let done_tasks = tasks.iter().filter(|t| t.is_done()).count() as u32;
        let pending_tasks = tasks.iter().filter(|t| t.is_pending()).count() as u32;
        let in_progress_tasks = tasks.iter().filter(|t| t.is_in_progress()).count() as u32;

        Self::new(total_tasks, done_tasks, pending_tasks, in_progress_tasks)
    }
}

impl From<&TaskList> for TaskStats {
    fn from(task_list: &TaskList) -> Self {
        Self::from_tasks(task_list.tasks())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TaskList {
    tasks: VecDeque<Task>,
    next_id: i32,
}

impl TaskList {
    pub fn tasks(&self) -> &VecDeque<Task> {
        &self.tasks
    }

    pub fn get_task_mut(&mut self, index: usize) -> Option<&mut Task> {
        self.tasks.get_mut(index)
    }
}

impl TaskList {
    pub fn new() -> Self {
        Self { tasks: VecDeque::new(), next_id: 1 }
    }

    pub fn append(&mut self, task: impl Into<TaskInput>) -> Task {
        let task: TaskInput = task.into();
        let mut new_task = Task::new(self.next_id, task.task);

        if let Some(category) = task.category {
            new_task = new_task.category(category);
        }

        if let Some(description) = task.note {
            new_task = new_task.note(description);
        }

        new_task = new_task.files(task.files.clone().unwrap_or_default());

        self.next_id += 1;
        self.tasks.push_back(new_task.clone());
        new_task
    }

    pub fn append_multiple(&mut self, tasks: Vec<TaskInput>) -> Vec<Task> {
        for task in tasks {
            self.append(task);
        }
        self.tasks.iter().cloned().collect::<Vec<_>>()
    }

    pub fn mark_done(&mut self, task_id: i32) -> Option<Task> {
        let task_index = self.tasks.iter().position(|t| t.id == task_id)?;
        self.tasks[task_index].mark_done();
        Some(self.tasks[task_index].clone())
    }

    pub fn update_status(&mut self, task_id: i32, status: Status) -> Option<Task> {
        if status == Status::Delete {
            return self.delete(task_id);
        }
        let task_index = self.tasks.iter().position(|t| t.id == task_id)?;
        self.tasks[task_index].status = status;
        Some(self.tasks[task_index].clone())
    }

    pub fn clear(&mut self) {
        self.tasks.clear();
        self.next_id = 1;
    }
    pub fn delete(&mut self, task_id: i32) -> Option<Task> {
        let task_index = self.tasks.iter().position(|t| t.id == task_id)?;
        self.tasks.remove(task_index)
    }
    /// Check if all tasks are completed (either Done or no tasks exist)
    pub fn all_tasks_done(&self) -> bool {
        self.tasks.is_empty() || self.tasks.iter().all(|task| task.is_done())
    }

    /// Find the next pending task (first task that is not Done)
    pub fn next_pending_task(&self) -> Option<&Task> {
        self.tasks.iter().find(|task| !task.is_done())
    }
}

impl Status {
    pub fn status_name(&self) -> &'static str {
        match self {
            Status::Pending => "PENDING",
            Status::InProgress => "IN_PROGRESS",
            Status::Done => "DONE",
            Status::Delete => "DELETE",
        }
    }
}

impl Task {
    pub fn status_name(&self) -> &'static str {
        self.status.status_name()
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_task_creation() {
        let task = Task::new(1, "Test task");

        assert_eq!(task.id, 1);
        assert_eq!(task.task, "Test task");
        assert_eq!(task.status, Status::Pending);
        assert!(task.is_pending());
    }

    #[test]
    fn test_task_status_transitions() {
        let mut task = Task::new(1, "Test task");

        task.mark_in_progress();
        assert!(task.is_in_progress());

        task.mark_done();
        assert!(task.is_done());
    }

    #[test]
    fn test_stats_from_tasks() {
        let tasks = vec![
            Task::new(1, "Task 1"), // Pending
            Task::new(2, "Task 2").status(Status::InProgress),
            Task::new(3, "Task 3").status(Status::Done),
            Task::new(4, "Task 4"), // Pending
        ]
        .into_iter()
        .collect();

        let stats = TaskStats::from_tasks(&tasks);

        assert_eq!(stats.total_tasks, 4);
        assert_eq!(stats.pending_tasks, 2);
        assert_eq!(stats.in_progress_tasks, 1);
        assert_eq!(stats.done_tasks, 1);
    }

    #[test]
    fn test_stats_from_task_list() {
        let mut task_list = TaskList::new();
        task_list.append("Task 1"); // Pending
        let task2 = task_list.append("Task 2");
        task_list.append("Task 3"); // Pending
        task_list.mark_done(task2.id); // Mark task 2 as done

        let stats = TaskStats::from(&task_list);

        assert_eq!(stats.total_tasks, 3);
        assert_eq!(stats.pending_tasks, 2);
        assert_eq!(stats.in_progress_tasks, 0);
        assert_eq!(stats.done_tasks, 1);
    }

    #[test]
    fn test_task_list_append() {
        let mut task_list = TaskList::new();

        let task = task_list.append("First task");

        assert_eq!(task.id, 1);
        assert_eq!(task.task, "First task");
        assert_eq!(task_list.tasks().len(), 1);
    }

    #[test]
    fn test_task_list_append_multiple() {
        let mut task_list = TaskList::new();
        let task_texts = vec![
            "Task 1".to_string(),
            "Task 2".to_string(),
            "Task 3".to_string(),
        ];

        let task_inputs: Vec<TaskInput> = task_texts.into_iter().map(|t| t.into()).collect();
        let created_tasks = task_list.append_multiple(task_inputs);

        assert_eq!(created_tasks.len(), 3);
        assert_eq!(created_tasks[0].id, 1);
        assert_eq!(created_tasks[0].task, "Task 1");
        assert_eq!(created_tasks[1].id, 2);
        assert_eq!(created_tasks[1].task, "Task 2");
        assert_eq!(created_tasks[2].id, 3);
        assert_eq!(created_tasks[2].task, "Task 3");
        assert_eq!(task_list.tasks().len(), 3);
    }

    #[test]
    fn test_task_list_append_multiple_empty() {
        let mut task_list = TaskList::new();
        let task_texts = vec![];

        let created_tasks = task_list.append_multiple(task_texts);

        assert_eq!(created_tasks.len(), 0);
        assert_eq!(task_list.tasks().len(), 0);
    }

    #[test]
    fn test_task_list_mark_done() {
        let mut task_list = TaskList::new();
        let task1 = task_list.append("Task 1");
        task_list.append("Task 2");

        let result = task_list.mark_done(task1.id);

        assert!(result.is_some());
        let completed_task = result.unwrap();
        assert_eq!(completed_task.task, "Task 1");
        assert!(completed_task.is_done());
    }

    #[test]
    fn test_task_list_mark_done_nonexistent() {
        let mut task_list = TaskList::new();
        task_list.append("Task 1");

        let result = task_list.mark_done(999);

        assert!(result.is_none());
    }

    #[test]
    fn test_task_list_clear() {
        let mut task_list = TaskList::new();
        task_list.append("Task 1");
        task_list.append("Task 2");

        task_list.clear();

        assert!(task_list.tasks().is_empty());
        assert_eq!(task_list.next_id, 1);
    }

    #[test]
    fn test_task_list_update_status() {
        let mut task_list = TaskList::new();
        let task1 = task_list.append("Task 1");
        task_list.append("Task 2");

        // Update to InProgress
        let result = task_list.update_status(task1.id, Status::InProgress);
        assert!(result.is_some());
        let updated_task = result.unwrap();
        assert_eq!(updated_task.task, "Task 1");
        assert!(updated_task.is_in_progress());

        // Update to Done
        let result = task_list.update_status(task1.id, Status::Done);
        assert!(result.is_some());
        let updated_task = result.unwrap();
        assert_eq!(updated_task.task, "Task 1");
        assert!(updated_task.is_done());

        // Update back to Pending
        let result = task_list.update_status(task1.id, Status::Pending);
        assert!(result.is_some());
        let updated_task = result.unwrap();
        assert_eq!(updated_task.task, "Task 1");
        assert!(updated_task.is_pending());
    }

    #[test]
    fn test_task_list_update_status_nonexistent() {
        let mut task_list = TaskList::new();
        task_list.append("Task 1");

        let result = task_list.update_status(999, Status::Done);

        assert!(result.is_none());
    }

    #[test]
    fn test_task_list_all_tasks_done_empty() {
        let task_list = TaskList::new();
        assert!(task_list.all_tasks_done());
    }

    #[test]
    fn test_task_list_all_tasks_done_with_completed_tasks() {
        let mut task_list = TaskList::new();
        let task1 = task_list.append("Task 1");
        let task2 = task_list.append("Task 2");

        task_list.mark_done(task1.id);
        task_list.mark_done(task2.id);

        assert!(task_list.all_tasks_done());
    }

    #[test]
    fn test_task_list_all_tasks_done_with_pending_tasks() {
        let mut task_list = TaskList::new();
        let task1 = task_list.append("Task 1");
        task_list.append("Task 2");

        task_list.mark_done(task1.id);

        assert!(!task_list.all_tasks_done());
    }

    #[test]
    fn test_task_list_next_pending_task_empty() {
        let task_list = TaskList::new();
        assert!(task_list.next_pending_task().is_none());
    }

    #[test]
    fn test_task_list_next_pending_task_with_pending() {
        let mut task_list = TaskList::new();
        let task1 = task_list.append("Task 1");
        let task2 = task_list.append("Task 2");

        task_list.mark_done(task1.id);

        let next_task = task_list.next_pending_task();
        assert!(next_task.is_some());
        assert_eq!(next_task.unwrap().id, task2.id);
        assert_eq!(next_task.unwrap().task, "Task 2");
    }

    #[test]
    fn test_task_list_next_pending_task_all_done() {
        let mut task_list = TaskList::new();
        let task1 = task_list.append("Task 1");
        let task2 = task_list.append("Task 2");

        task_list.mark_done(task1.id);
        task_list.mark_done(task2.id);

        assert!(task_list.next_pending_task().is_none());
    }
    #[test]
    fn test_task_list_delete() {
        let mut task_list = TaskList::new();
        let task1 = task_list.append("Task 1");
        let task2 = task_list.append("Task 2");
        let task3 = task_list.append("Task 3");

        // Delete the middle task
        let result = task_list.delete(task2.id);

        assert!(result.is_some());
        let deleted_task = result.unwrap();
        assert_eq!(deleted_task.id, task2.id);
        assert_eq!(deleted_task.task, "Task 2");
        assert_eq!(task_list.tasks().len(), 2);

        // Verify remaining tasks
        let remaining_ids: Vec<i32> = task_list.tasks().iter().map(|t| t.id).collect();
        assert_eq!(remaining_ids, vec![task1.id, task3.id]);
    }

    #[test]
    fn test_task_list_delete_nonexistent() {
        let mut task_list = TaskList::new();
        task_list.append("Task 1");

        let result = task_list.delete(999);

        assert!(result.is_none());
        assert_eq!(task_list.tasks().len(), 1);
    }

    #[test]
    fn test_update_status_with_delete() {
        let mut task_list = TaskList::new();
        let task1 = task_list.append("Task 1");
        let task2 = task_list.append("Task 2");
        let task3 = task_list.append("Task 3");

        // Update status to Delete should remove the task
        let deleted_task = task_list.update_status(task2.id, Status::Delete);

        assert!(deleted_task.is_some());
        let deleted_task = deleted_task.unwrap();
        assert_eq!(deleted_task.id, task2.id);
        assert_eq!(deleted_task.task, "Task 2");
        assert_eq!(task_list.tasks().len(), 2);

        // Verify remaining tasks
        let remaining_ids: Vec<i32> = task_list.tasks().iter().map(|t| t.id).collect();
        assert_eq!(remaining_ids, vec![task1.id, task3.id]);
    }

    #[test]
    fn test_status_name_delete() {
        let status = Status::Delete;
        assert_eq!(status.status_name(), "DELETE");
    }
}
#[test]
fn test_task_creation_with_category_and_description() {
    let mut task = Task::new(1, "Test task");
    task.category = Some("Development".to_string());
    task.note = Some("Implement user authentication with OAuth2. Example: Add login/logout endpoints in auth.rs, update user model in models/user.rs".to_string());

    assert_eq!(task.id, 1);
    assert_eq!(task.task, "Test task");
    assert_eq!(task.status, Status::Pending);
    assert_eq!(task.category, Some("Development".to_string()));
    assert_eq!(task.note, Some("Implement user authentication with OAuth2. Example: Add login/logout endpoints in auth.rs, update user model in models/user.rs".to_string()));
    assert!(task.is_pending());
}

#[test]
fn test_task_creation_with_setters() {
    let task = Task::new(1, "Test task").category("Bug Fix").note(
        "Fix memory leak in connection pool. Files: src/db/pool.rs, tests/integration/db_test.rs",
    );

    assert_eq!(task.id, 1);
    assert_eq!(task.task, "Test task");
    assert_eq!(task.category, Some("Bug Fix".to_string()));
    assert_eq!(task.note, Some("Fix memory leak in connection pool. Files: src/db/pool.rs, tests/integration/db_test.rs".to_string()));
}

#[test]
fn test_task_list_append_with_details() {
    let mut task_list = TaskList::new();

    let task = task_list.append(TaskInput {
        task: "Implement feature X".to_string(),
        category: Some("Feature".to_string()),
        note: Some("Add new API endpoint for user preferences. Example: POST /api/v1/users/{id}/preferences. Files: src/handlers/user.rs, src/models/preference.rs".to_string()),
        files: None,
    });

    assert_eq!(task.id, 1);
    assert_eq!(task.task, "Implement feature X");
    assert_eq!(task.category, Some("Feature".to_string()));
    assert_eq!(task.note, Some("Add new API endpoint for user preferences. Example: POST /api/v1/users/{id}/preferences. Files: src/handlers/user.rs, src/models/preference.rs".to_string()));
    assert_eq!(task_list.tasks().len(), 1);
}

#[test]
fn test_task_list_append_multiple_with_details() {
    let mut task_list = TaskList::new();
    let task_inputs = vec![
        TaskInput {
            task: "Task 1".to_string(),
            category: Some("Development".to_string()),
            note: Some("Implement login functionality. Files: src/auth/login.rs".to_string()),
            files: None,
        },
        TaskInput {
            task: "Task 2".to_string(),
            category: Some("Testing".to_string()),
            note: Some(
                "Write unit tests for user service. Files: tests/user_service_test.rs".to_string(),
            ),
            files: None,
        },
        TaskInput {
            task: "Task 3".to_string(),
            category: None,
            note: None,
            files: None,
        },
    ];

    let created_tasks = task_list.append_multiple(task_inputs);

    assert_eq!(created_tasks.len(), 3);
    assert_eq!(created_tasks[0].id, 1);
    assert_eq!(created_tasks[0].task, "Task 1");
    assert_eq!(created_tasks[0].category, Some("Development".to_string()));
    assert_eq!(
        created_tasks[0].note,
        Some("Implement login functionality. Files: src/auth/login.rs".to_string())
    );

    assert_eq!(created_tasks[1].id, 2);
    assert_eq!(created_tasks[1].task, "Task 2");
    assert_eq!(created_tasks[1].category, Some("Testing".to_string()));
    assert_eq!(
        created_tasks[1].note,
        Some("Write unit tests for user service. Files: tests/user_service_test.rs".to_string())
    );

    assert_eq!(created_tasks[2].id, 3);
    assert_eq!(created_tasks[2].task, "Task 3");
    assert_eq!(created_tasks[2].category, None);
    assert_eq!(created_tasks[2].note, None);

    assert_eq!(task_list.tasks().len(), 3);
}
