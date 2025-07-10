use std::collections::VecDeque;

use derive_setters::Setters;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, eserde::Deserialize, Default, JsonSchema)]
pub enum Status {
    #[default]
    Pending,
    InProgress,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
pub struct Task {
    pub id: i32,
    pub task: String,
    pub status: Status,
}

impl Task {
    pub fn new(id: i32, task: impl Into<String>) -> Self {
        Self { id, task: task.into(), status: Status::default() }
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

    pub fn append(&mut self, task: impl Into<String>) -> Task {
        let task = Task::new(self.next_id, task);
        self.next_id += 1;
        self.tasks.push_back(task.clone());
        task
    }

    pub fn append_multiple(&mut self, tasks: Vec<String>) -> Vec<Task> {
        let mut created_tasks = Vec::new();
        for task_text in tasks {
            let task = self.append(task_text);
            created_tasks.push(task);
        }
        created_tasks
    }

    pub fn mark_done(&mut self, task_id: i32) -> Option<Task> {
        let task_index = self.tasks.iter().position(|t| t.id == task_id)?;
        self.tasks[task_index].mark_done();
        Some(self.tasks[task_index].clone())
    }

    pub fn update_status(&mut self, task_id: i32, status: Status) -> Option<Task> {
        let task_index = self.tasks.iter().position(|t| t.id == task_id)?;
        self.tasks[task_index].status = status;
        Some(self.tasks[task_index].clone())
    }

    pub fn clear(&mut self) {
        self.tasks.clear();
        self.next_id = 1;
    }
}

impl Status {
    pub fn status_name(&self) -> &'static str {
        match self {
            Status::Pending => "PENDING",
            Status::InProgress => "IN_PROGRESS",
            Status::Done => "DONE",
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

        let created_tasks = task_list.append_multiple(task_texts);

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
}
