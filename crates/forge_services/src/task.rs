use std::sync::Arc;

use anyhow::Result;
use forge_app::TaskService;
use forge_domain::{Stats, Task, TaskId, TaskListResult, TaskStatus};
use tokio::sync::Mutex;

/// Service for managing tasks, including creation, status updates, and
/// retrieval
#[derive(Clone)]
pub struct ForgeTaskService {
    tasks: Arc<Mutex<Vec<Task>>>,
}

impl Default for ForgeTaskService {
    fn default() -> Self {
        Self::new()
    }
}

impl ForgeTaskService {
    /// Creates a new ForgeTaskService with the provided infrastructure
    pub fn new() -> Self {
        Self { tasks: Arc::new(Mutex::new(Vec::new())) }
    }

    /// Calculate statistics for the current task list.
    async fn calculate_stats(&self) -> Stats {
        let tasks = self.tasks.lock().await;
        let total_tasks = tasks.len() as u32;
        let done_tasks = tasks
            .iter()
            .filter(|task| task.status == TaskStatus::Done)
            .count() as u32;
        let in_progress_tasks = tasks
            .iter()
            .filter(|task| task.status == TaskStatus::InProgress)
            .count() as u32;
        let pending_tasks = tasks
            .iter()
            .filter(|task| task.status == TaskStatus::Pending)
            .count() as u32;

        Stats { total_tasks, done_tasks, pending_tasks, in_progress_tasks }
    }

    /// Helper method to create a TaskListResult with current stats
    pub async fn create_result(
        &self,
        message: Option<String>,
        next_task: Option<Task>,
        tasks: Option<Vec<Task>>,
    ) -> Result<TaskListResult> {
        let stats = self.calculate_stats().await;
        Ok(TaskListResult { message, next_task, stats, tasks })
    }
}

#[async_trait::async_trait]
impl TaskService for ForgeTaskService {
    /// Appends multiple tasks to the end of the task list atomically
    async fn append(&self, descriptions: Vec<String>) -> Result<()> {
        if descriptions.is_empty() {
            return Err(anyhow::anyhow!("Cannot append empty list of tasks"));
        }

        let mut tasks_guard = self.tasks.lock().await;
        let mut existing_ids: Vec<TaskId> = tasks_guard.iter().map(|t| t.id.clone()).collect();
        for description in descriptions {
            let next_id = TaskId::next(&existing_ids);
            existing_ids.push(next_id.clone());
            tasks_guard.push(Task { id: next_id, description, status: TaskStatus::Pending });
        }
        Ok(())
    }

    /// Prepends multiple tasks to the beginning of the task list atomically
    async fn prepend(&self, descriptions: Vec<String>) -> Result<()> {
        if descriptions.is_empty() {
            return Err(anyhow::anyhow!("Cannot prepend empty list of tasks"));
        }

        let mut tasks_guard = self.tasks.lock().await;
        let mut existing_ids: Vec<TaskId> = tasks_guard.iter().map(|t| t.id.clone()).collect();
        for (i, description) in descriptions.into_iter().enumerate() {
            let next_id = TaskId::next(&existing_ids);
            existing_ids.push(next_id.clone());
            tasks_guard.insert(
                i,
                Task { id: next_id, description, status: TaskStatus::Pending },
            );
        }
        Ok(())
    }

    /// Marks the first pending task as in progress and returns it
    async fn pop_front(&self) -> Result<Option<Task>> {
        let task_option = {
            let mut tasks = self.tasks.lock().await;
            if tasks.is_empty() {
                None
            } else {
                let pending_index = tasks
                    .iter()
                    .position(|task| task.status == TaskStatus::Pending);

                if let Some(index) = pending_index {
                    tasks[index].status = TaskStatus::InProgress;
                    Some(tasks[index].clone())
                } else {
                    None
                }
            }
        };

        Ok(task_option)
    }

    /// Marks the last pending task as in progress and returns it
    async fn pop_back(&self) -> Result<Option<Task>> {
        let task_option = {
            let mut tasks = self.tasks.lock().await;
            if tasks.is_empty() {
                None
            } else {
                let pending_task_index = tasks
                    .iter()
                    .rposition(|task| task.status == TaskStatus::Pending);

                if let Some(index) = pending_task_index {
                    tasks[index].status = TaskStatus::InProgress;
                    Some(tasks[index].clone())
                } else {
                    None
                }
            }
        };

        Ok(task_option)
    }

    /// Marks a task as done by its ID
    async fn mark_done(&self, id: TaskId) -> Result<Option<Task>> {
        let found_task = {
            let mut tasks = self.tasks.lock().await;
            let mut found_task = None;

            for task in tasks.iter_mut() {
                if task.id == id {
                    task.status = TaskStatus::Done;
                    found_task = Some(task.clone());
                    break;
                }
            }

            found_task
        };

        Ok(found_task)
    }

    /// Lists all tasks in the current task list
    async fn list(&self) -> Result<Vec<Task>> {
        let tasks = self.tasks.lock().await;
        Ok(tasks.clone())
    }

    /// Gets statistics about the current task list
    async fn stats(&self) -> Result<(u32, u32, u32, u32)> {
        let stats = self.calculate_stats().await;
        Ok((
            stats.total_tasks,
            stats.done_tasks,
            stats.pending_tasks,
            stats.in_progress_tasks,
        ))
    }

    /// Finds the next pending task
    async fn find_next_pending(&self) -> Result<Option<Task>> {
        let tasks = self.tasks.lock().await;
        Ok(tasks
            .iter()
            .find(|task| task.status == TaskStatus::Pending)
            .cloned())
    }

    /// Clears all tasks from the task list
    async fn clear(&self) -> Result<()> {
        let mut tasks = self.tasks.lock().await;
        tasks.clear();
        Ok(())
    }

    /// Formats tasks as markdown
    async fn format_markdown(&self) -> Result<String> {
        let tasks = self.list().await?;
        let (total_tasks, done_tasks, pending_tasks, in_progress_tasks) = self.stats().await?;

        // Generate markdown format
        let mut markdown = String::from("# Task List\n\n");

        if tasks.is_empty() {
            markdown.push_str("*No tasks in the list.*\n\n");
        } else {
            // Group tasks by status
            let pending_tasks_list: Vec<_> = tasks
                .iter()
                .filter(|t| matches!(t.status, TaskStatus::Pending))
                .collect();
            let in_progress_tasks_list: Vec<_> = tasks
                .iter()
                .filter(|t| matches!(t.status, TaskStatus::InProgress))
                .collect();
            let done_tasks_list: Vec<_> = tasks
                .iter()
                .filter(|t| matches!(t.status, TaskStatus::Done))
                .collect();

            if !pending_tasks_list.is_empty() {
                markdown.push_str("## 📋 Pending Tasks\n\n");
                for (i, task) in pending_tasks_list.iter().enumerate() {
                    markdown.push_str(&format!(
                        "{}. [ ] {} `{}`\n",
                        i + 1,
                        task.description,
                        task.id
                    ));
                }
                markdown.push('\n');
            }

            if !in_progress_tasks_list.is_empty() {
                markdown.push_str("## 🚧 In Progress\n\n");
                for (i, task) in in_progress_tasks_list.iter().enumerate() {
                    markdown.push_str(&format!(
                        "{}. [⚡] {} `{}`\n",
                        i + 1,
                        task.description,
                        task.id
                    ));
                }
                markdown.push('\n');
            }

            if !done_tasks_list.is_empty() {
                markdown.push_str("## ✅ Completed Tasks\n\n");
                for (i, task) in done_tasks_list.iter().enumerate() {
                    markdown.push_str(&format!(
                        "{}. [x] {} `{}`\n",
                        i + 1,
                        task.description,
                        task.id
                    ));
                }
                markdown.push('\n');
            }
        }

        // Add summary
        markdown.push_str("## 📊 Summary\n\n");
        markdown.push_str(&format!("- **Total Tasks:** {total_tasks}\n"));
        markdown.push_str(&format!("- **Pending:** {pending_tasks}\n"));
        markdown.push_str(&format!("- **In Progress:** {in_progress_tasks}\n"));
        markdown.push_str(&format!("- **Completed:** {done_tasks}\n"));

        Ok(markdown)
    }
}

#[cfg(test)]
pub mod tests {
    use std::sync::Arc;

    use pretty_assertions::assert_eq;

    use super::*;

    #[derive(Clone, Debug)]
    pub struct MockTaskService {
        tasks: Arc<Mutex<Vec<Task>>>,
        counter: Arc<Mutex<u8>>,
    }

    impl Default for MockTaskService {
        fn default() -> Self {
            Self::new()
        }
    }

    impl MockTaskService {
        pub fn new() -> Self {
            Self {
                tasks: Arc::new(Mutex::new(Vec::new())),
                counter: Arc::new(Mutex::new(0)),
            }
        }

        fn create_deterministic_task(&self, description: String, counter: u8) -> Task {
            Task {
                id: TaskId::from_u64(counter as u64),
                description,
                status: TaskStatus::default(),
            }
        }
    }

    #[async_trait::async_trait]
    impl TaskService for MockTaskService {
        async fn append(&self, descriptions: Vec<String>) -> anyhow::Result<()> {
            if descriptions.is_empty() {
                return Err(anyhow::anyhow!("Cannot append empty list of tasks"));
            }

            let mut tasks = self.tasks.lock().await;
            let mut counter = self.counter.lock().await;

            let new_tasks: Vec<Task> = descriptions
                .into_iter()
                .map(|desc| {
                    let task = self.create_deterministic_task(desc, *counter);
                    *counter += 1;
                    task
                })
                .collect();

            tasks.extend(new_tasks);
            Ok(())
        }

        async fn prepend(&self, descriptions: Vec<String>) -> anyhow::Result<()> {
            if descriptions.is_empty() {
                return Err(anyhow::anyhow!("Cannot prepend empty list of tasks"));
            }

            let mut tasks = self.tasks.lock().await;
            let mut counter = self.counter.lock().await;

            let new_tasks: Vec<Task> = descriptions
                .into_iter()
                .map(|desc| {
                    let task = self.create_deterministic_task(desc, *counter);
                    *counter += 1;
                    task
                })
                .collect();

            // Insert in reverse order to maintain the order of the input
            for (i, task) in new_tasks.into_iter().enumerate() {
                tasks.insert(i, task);
            }
            Ok(())
        }

        async fn pop_front(&self) -> anyhow::Result<Option<Task>> {
            let mut tasks = self.tasks.lock().await;
            if tasks.is_empty() {
                Ok(None)
            } else {
                let pending_index = tasks
                    .iter()
                    .position(|task| task.status == TaskStatus::Pending);

                if let Some(index) = pending_index {
                    tasks[index].status = TaskStatus::InProgress;
                    Ok(Some(tasks[index].clone()))
                } else {
                    Ok(None)
                }
            }
        }

        async fn pop_back(&self) -> anyhow::Result<Option<Task>> {
            let mut tasks = self.tasks.lock().await;
            if tasks.is_empty() {
                Ok(None)
            } else {
                let pending_index = tasks
                    .iter()
                    .rposition(|task| task.status == TaskStatus::Pending);

                if let Some(index) = pending_index {
                    tasks[index].status = TaskStatus::InProgress;
                    Ok(Some(tasks[index].clone()))
                } else {
                    Ok(None)
                }
            }
        }

        async fn mark_done(&self, id: TaskId) -> anyhow::Result<Option<Task>> {
            let mut tasks = self.tasks.lock().await;
            for task in tasks.iter_mut() {
                if task.id == id {
                    task.status = TaskStatus::Done;
                    return Ok(Some(task.clone()));
                }
            }
            Ok(None)
        }

        async fn list(&self) -> anyhow::Result<Vec<Task>> {
            let tasks = self.tasks.lock().await;
            Ok(tasks.clone())
        }

        async fn stats(&self) -> anyhow::Result<(u32, u32, u32, u32)> {
            let tasks = self.tasks.lock().await;
            let total = tasks.len() as u32;
            let done = tasks
                .iter()
                .filter(|t| t.status == TaskStatus::Done)
                .count() as u32;
            let pending = tasks
                .iter()
                .filter(|t| t.status == TaskStatus::Pending)
                .count() as u32;
            let in_progress = tasks
                .iter()
                .filter(|t| t.status == TaskStatus::InProgress)
                .count() as u32;
            Ok((total, done, pending, in_progress))
        }

        async fn find_next_pending(&self) -> anyhow::Result<Option<Task>> {
            let tasks = self.tasks.lock().await;
            Ok(tasks
                .iter()
                .find(|t| t.status == TaskStatus::Pending)
                .cloned())
        }

        async fn format_markdown(&self) -> anyhow::Result<String> {
            let tasks = self.tasks.lock().await;

            if tasks.is_empty() {
                return Ok("No tasks available.".to_string());
            }

            let mut markdown = String::new();
            let mut sorted_tasks = tasks.clone();
            sorted_tasks.sort_by_key(|task| task.id.clone());

            for task in sorted_tasks {
                let checkbox = match task.status {
                    TaskStatus::Done => "[x]",
                    TaskStatus::Pending => "[ ]",
                    TaskStatus::InProgress => "[ ]",
                };

                let formatted_task = match task.status {
                    TaskStatus::Done => {
                        format!("- {} {}", checkbox, task.description)
                    }
                    TaskStatus::Pending => {
                        format!("- {} {}", checkbox, task.description)
                    }
                    TaskStatus::InProgress => {
                        format!("- {} __{} (In Progress)__ ", checkbox, task.description)
                    }
                };

                markdown.push_str(&formatted_task);
                markdown.push('\n');
            }

            Ok(markdown)
        }

        async fn clear(&self) -> anyhow::Result<()> {
            let mut tasks = self.tasks.lock().await;
            let mut counter = self.counter.lock().await;
            tasks.clear();
            *counter = 0;
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_clear_tasks() {
        // Fixture: Create task service with multiple tasks
        let service = ForgeTaskService::new();
        service
            .append(vec![
                "Task 1".to_string(),
                "Task 2".to_string(),
                "Task 3".to_string(),
            ])
            .await
            .unwrap();

        // Verify tasks exist
        let tasks_before = service.list().await.unwrap();
        assert_eq!(tasks_before.len(), 3);

        // Actual: Clear all tasks
        let actual = service.clear().await;

        // Expected: Operation should succeed and all tasks should be removed
        assert!(actual.is_ok());
        let tasks_after = service.list().await.unwrap();
        assert_eq!(tasks_after.len(), 0);

        // Verify stats are updated
        let stats = service.stats().await.unwrap();
        assert_eq!(stats, (0, 0, 0, 0)); // (total, done, pending, in_progress)
    }

    #[tokio::test]
    async fn test_append_task() {
        // Fixture: Create task service
        let service = ForgeTaskService::new();

        // Actual: Append a task
        let actual = service.append(vec!["Test task".to_string()]).await;

        // Expected: Operation should succeed
        assert!(actual.is_ok());

        // Verify task was added
        let tasks = service.list().await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].description, "Test task");
        assert_eq!(tasks[0].status, TaskStatus::Pending);
    }

    #[tokio::test]
    async fn test_prepend_task() {
        // Fixture: Create task service with existing task
        let service = ForgeTaskService::new();
        service.append(vec!["Task 1".to_string()]).await.unwrap();

        // Actual: Prepend a task
        service.prepend(vec!["Task 2".to_string()]).await.unwrap();

        // Expected: Task 2 should be first
        let tasks = service.list().await.unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].description, "Task 2");
        assert_eq!(tasks[1].description, "Task 1");
    }

    #[tokio::test]
    async fn test_pop_front() {
        // Fixture: Create task service with tasks
        let service = ForgeTaskService::new();
        service
            .append(vec!["Task to pop".to_string()])
            .await
            .unwrap();

        // Actual: Pop front task
        let actual = service.pop_front().await.unwrap();

        // Expected: Task should be returned and marked in progress
        assert!(actual.is_some());
        let task = actual.unwrap();
        assert_eq!(task.description, "Task to pop");
        assert_eq!(task.status, TaskStatus::InProgress);
    }

    #[tokio::test]
    async fn test_mark_done() {
        // Fixture: Create task service with task
        let service = ForgeTaskService::new();
        service
            .append(vec!["Task to complete".to_string()])
            .await
            .unwrap();
        let tasks = service.list().await.unwrap();
        let task_id = tasks[0].id.clone();

        // Actual: Mark task as done
        let actual = service.mark_done(task_id).await.unwrap();

        // Expected: Task should be marked as done
        assert!(actual.is_some());
        let completed_task = actual.unwrap();
        assert_eq!(completed_task.status, TaskStatus::Done);
    }

    #[tokio::test]
    async fn test_stats() {
        // Fixture: Create task service with various tasks
        let service = ForgeTaskService::new();
        service
            .append(vec![
                "Pending task".to_string(),
                "Task to progress".to_string(),
                "Task to complete".to_string(),
            ])
            .await
            .unwrap();

        // Set up different statuses
        service.pop_front().await.unwrap(); // Mark first as in progress
        let tasks = service.list().await.unwrap();
        service.mark_done(tasks[2].id.clone()).await.unwrap(); // Mark last as done

        // Actual: Get stats
        let actual = service.stats().await.unwrap();

        // Expected: Should have correct counts
        let expected = (3, 1, 1, 1); // (total, done, pending, in_progress)
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_format_markdown() {
        // Fixture: Create task service with tasks
        let service = ForgeTaskService::new();
        service
            .append(vec![
                "Pending task".to_string(),
                "In progress task".to_string(),
            ])
            .await
            .unwrap();
        service.pop_back().await.unwrap(); // Mark second as in progress

        // Actual: Format as markdown
        let actual = service.format_markdown().await.unwrap();

        // Expected: Should contain structured markdown format
        assert!(actual.contains("# Task List"));
        assert!(actual.contains("## 📋 Pending Tasks"));
        assert!(actual.contains("1. [ ] Pending task"));
        assert!(actual.contains("## 🚧 In Progress"));
        assert!(actual.contains("1. [⚡] In progress task"));
    }
}
