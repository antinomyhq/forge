use std::fmt::Display;
use std::sync::Arc;

use anyhow::Result;
use derive_setters::Setters;
use forge_display::TitleFormat;
use forge_domain::{
    EnvironmentService, ExecutableTool, NamedTool, TaskListInput, ToolCallContext, ToolDescription,
    ToolName,
};
use forge_tool_macros::ToolDescription;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{FsWriteService, Infrastructure};

/// Represents the status of a task in the TaskList.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub enum TaskStatus {
    /// Task is waiting to be started.
    Pending,
    /// Task is currently being worked on.
    InProgress,
    /// Task has been completed.
    Done,
}

impl Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Done => write!(f, "Done"),
            TaskStatus::InProgress => write!(f, "InProgress"),
            TaskStatus::Pending => write!(f, "Pending"),
        }
    }
}

/// Represents a single task in the TaskList.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Setters)]
#[setters(into, strip_option)]
pub struct Task {
    /// Unique identifier for the task.
    pub id: u32,
    /// Description of the task.
    pub task: String,
    /// Current status of the task.
    pub status: TaskStatus,
}

impl Display for Task {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "<task id=\"{}\">\n<content>{}</content>\n<status>{}</status>\n</task>",
            self.id, self.task, self.status
        )
    }
}

/// Statistics about the TaskList.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Stats {
    /// Total number of tasks in the list.
    pub total_tasks: u32,
    /// Number of completed tasks.
    pub done_tasks: u32,
    /// Number of pending tasks.
    pub pending_tasks: u32,
    /// Number of in-progress tasks.
    pub in_progress_tasks: u32,
}

impl std::fmt::Display for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<stats>\n<total_tasks>{}</total_tasks>\n<done_tasks>{}</done_tasks>\n<pending_tasks>{}</pending_tasks>\n<in_progress_tasks>{}</in_progress_tasks>\n</stats>",
            self.total_tasks, self.done_tasks, self.pending_tasks, self.in_progress_tasks)
    }
}

/// TaskList operation result.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TaskListResult {
    /// Message describing the result of the operation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// The task that was affected by the operation, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_task: Option<Task>,
    /// Statistics about the task list.
    pub stats: Stats,
    /// List of tasks
    pub tasks: Option<Vec<Task>>,
}

impl Display for TaskListResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut result = String::from("<task_list_result>\n");

        if let Some(message) = &self.message {
            result.push_str(&format!("<message>{message}</message>\n"));
        }

        if let Some(task) = &self.next_task {
            result.push_str("<next_task>\n");
            result.push_str(&format!("{task}\n"));
            result.push_str("\n</next_task>\n");
        }

        result.push_str(&format!("{}\n", self.stats));

        if let Some(tasks) = &self.tasks {
            if !tasks.is_empty() {
                result.push_str("<tasks_list>\n");
                for task in tasks {
                    result.push_str(&format!("{task}\n"));
                }
                result.push_str("</tasks_list>\n");
            }
        }

        result.push_str("</task_list_result>");

        write!(f, "{result}")
    }
}

/// A stateful task management tool that maintains an ordered list of tasks with
/// status tracking. Provides operations to add tasks (append/prepend), mark
/// tasks as in-progress (pop_front/pop_back), complete tasks (mark_done), and
/// view the current state (list). Automatically identifies the next pending
/// task when completing items and provides detailed statistics on task status.
/// Ideal for sequential workflows, project planning, and tracking multi-step
/// processes.
#[derive(Debug, ToolDescription)]
pub struct TaskList<F> {
    infra: Arc<F>,
    tasks: Arc<Mutex<Vec<Task>>>,
    next_id: Arc<Mutex<u32>>,
}

impl<F: Infrastructure> TaskList<F> {
    /// Creates a new TaskList tool with the given infrastructure.
    pub fn new(infra: Arc<F>) -> Self {
        Self {
            infra,
            tasks: Arc::new(Mutex::new(Vec::new())),
            next_id: Arc::new(Mutex::new(1)),
        }
    }

    /// Format the tasks as a markdown list with checkboxes
    pub async fn format_tasks_markdown(&self) -> String {
        let tasks = self.tasks.lock().await;

        if tasks.is_empty() {
            return "No tasks available.".to_string();
        }

        let mut markdown = String::new();

        // Sort tasks by ID for consistent display
        let mut sorted_tasks = tasks.clone();
        sorted_tasks.sort_by_key(|task| task.id);

        for task in sorted_tasks {
            let checkbox = match task.status {
                TaskStatus::Done => "[x]",
                TaskStatus::Pending => "[ ]",
                TaskStatus::InProgress => "[ ]",
            };

            let formatted_task = match task.status {
                TaskStatus::Done => format!("- {} {}", checkbox, task.task),
                TaskStatus::Pending => format!("- {} {}", checkbox, task.task),
                TaskStatus::InProgress => {
                    format!("- {} __{} (In Progress)__ ", checkbox, task.task)
                }
            };

            markdown.push_str(&formatted_task);
            markdown.push('\n');
        }

        markdown
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

    /// Get the next available task ID and increment the counter.
    async fn get_next_id(&self) -> u32 {
        let mut next_id = self.next_id.lock().await;
        let id = *next_id;
        *next_id += 1;
        id
    }

    /// Find the next pending task in the list.
    async fn find_next_pending_task(&self) -> Option<Task> {
        let tasks = self.tasks.lock().await;
        tasks
            .iter()
            .find(|task| task.status == TaskStatus::Pending)
            .cloned()
    }

    /// Handle the append operation, adding a task to the end of the list.
    async fn append(&self, task_desc: String) -> Result<TaskListResult> {
        let task = Task {
            id: self.get_next_id().await,
            task: task_desc,
            status: TaskStatus::Pending,
        };

        {
            let mut tasks = self.tasks.lock().await;
            tasks.push(task.clone());
        }

        let stats = self.calculate_stats().await;

        Ok(TaskListResult {
            message: Some("Task added to the end of the list.".to_string()),
            next_task: None,
            stats,
            tasks: None,
        })
    }

    /// Handle the prepend operation, adding a task to the beginning of the
    /// list.
    async fn prepend(&self, task_desc: String) -> Result<TaskListResult> {
        let task = Task {
            id: self.get_next_id().await,
            task: task_desc,
            status: TaskStatus::Pending,
        };

        {
            let mut tasks = self.tasks.lock().await;
            tasks.insert(0, task.clone());
        }

        let stats = self.calculate_stats().await;

        Ok(TaskListResult {
            message: Some("Task added to the beginning of the list.".to_string()),
            next_task: None,
            stats,
            tasks: None,
        })
    }

    /// Handle the pop_front operation, marking the first pending task as
    /// InProgress without removing it.
    async fn pop_front(&self) -> Result<TaskListResult> {
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

        let stats = self.calculate_stats().await;

        if let Some(task) = task_option {
            Ok(TaskListResult { message: None, next_task: Some(task), stats, tasks: None })
        } else {
            Ok(TaskListResult {
                message: Some("No pending tasks found.".to_string()),
                next_task: None,
                stats: self.calculate_stats().await,
                tasks: None,
            })
        }
    }

    /// Handle the pop_back operation, marking the last pending task as
    /// InProgress without removing it.
    async fn pop_back(&self) -> Result<TaskListResult> {
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

        let stats = self.calculate_stats().await;

        if let Some(task) = task_option {
            Ok(TaskListResult { message: None, next_task: Some(task), stats, tasks: None })
        } else {
            Ok(TaskListResult {
                message: Some("No pending tasks found.".to_string()),
                next_task: None,
                stats,
                tasks: None,
            })
        }
    }

    /// Handle the mark_done operation, marking a task as completed and finding
    /// the next pending task.
    async fn mark_done(&self, id: u32) -> Result<TaskListResult> {
        let found = {
            let mut tasks = self.tasks.lock().await;
            let mut found = false;

            for task in tasks.iter_mut() {
                if task.id == id {
                    task.status = TaskStatus::Done;
                    found = true;
                    break;
                }
            }

            found
        };

        if !found {
            return Ok(TaskListResult {
                message: Some(format!("No task found with ID {id}.")),
                next_task: None,
                stats: self.calculate_stats().await,
                tasks: None,
            });
        }

        let next_task = self.find_next_pending_task().await;
        let stats = self.calculate_stats().await;

        Ok(TaskListResult {
            message: if next_task.is_some() {
                Some(format!(
                    "Task {} marked as done. Next taskId is {}.",
                    id,
                    next_task.as_ref().unwrap().id
                ))
            } else {
                Some(format!("Task {id} marked as done. No more pending tasks."))
            },
            next_task,
            stats,
            tasks: None,
        })
    }

    /// Handle the list operation, returning all tasks in the list.
    async fn list(&self) -> Result<TaskListResult> {
        let stats = self.calculate_stats().await;
        let tasks = self.tasks.lock().await;
        Ok(TaskListResult {
            message: None,
            next_task: None,
            stats,
            tasks: Some(tasks.clone()),
        })
    }
}

impl<F> NamedTool for TaskList<F> {
    fn tool_name() -> ToolName {
        ToolName::new("forge_tool_task_list")
    }
}

fn format_input(input: &TaskListInput) -> String {
    if let Some(task) = &input.append_task {
        return format!("Append: {task}");
    } else if let Some(task) = &input.prepend_task {
        return format!("Prepend: {task}");
    } else if input.pop_front.is_some() {
        return "PopFront".to_string();
    } else if input.pop_back.is_some() {
        return "PopBack".to_string();
    } else if let Some(id) = input.mark_done_id {
        return format!("MarkDone: {id}");
    } else if input.list.is_some() {
        return "List".to_string();
    }

    "Unknown operation".to_string()
}

#[async_trait::async_trait]
impl<F: Infrastructure> ExecutableTool for TaskList<F> {
    type Input = TaskListInput;

    async fn call(&self, mut context: ToolCallContext, input: Self::Input) -> Result<String> {
        context
            .send_text(TitleFormat::debug("TaskList").sub_title(format_input(&input)))
            .await?;

        let result = if let Some(task) = input.append_task {
            self.append(task).await?
        } else if let Some(task) = input.prepend_task {
            self.prepend(task).await?
        } else if input.pop_front.is_some() {
            self.pop_front().await?
        } else if input.pop_back.is_some() {
            self.pop_back().await?
        } else if let Some(id) = input.mark_done_id {
            self.mark_done(id).await?
        } else if input.list.is_some() {
            self.list().await?
        } else {
            return Err(anyhow::anyhow!("No valid operation specified"));
        };

        // Format tasks in markdown format
        let markdown = self.format_tasks_markdown().await;
        let cwd = self
            .infra
            .environment_service()
            .get_environment()
            .cwd
            .join("task_list.md");
        self.infra
            .file_write_service()
            .write(cwd.as_path(), markdown.into())
            .await?;

        context
            .set_stats(self.calculate_stats().await.to_string())
            .await;

        Ok(result.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use forge_domain::ToolCallContext;

    use super::*;
    use crate::attachment::tests::MockInfrastructure;

    fn create_task_list() -> TaskList<MockInfrastructure> {
        let infra = Arc::new(MockInfrastructure::new());
        TaskList::new(infra)
    }

    #[tokio::test]
    async fn test_append_task() {
        // Create fixture
        let task_list = create_task_list();
        let input = TaskListInput {
            append_task: Some("Test task".to_string()),
            prepend_task: None,
            pop_front: None,
            pop_back: None,
            mark_done_id: None,
            list: None,
        };

        // Execute the fixture
        let result = task_list
            .call(ToolCallContext::default(), input)
            .await
            .unwrap();

        insta::assert_snapshot!(result);
    }

    #[tokio::test]
    async fn test_prepend_task() {
        // Create fixture
        let task_list = create_task_list();

        // Add two tasks
        let input1 = TaskListInput {
            append_task: Some("Task 1".to_string()),
            prepend_task: None,
            pop_front: None,
            pop_back: None,
            mark_done_id: None,
            list: None,
        };
        let input2 = TaskListInput {
            append_task: None,
            prepend_task: Some("Task 2".to_string()),
            pop_front: None,
            pop_back: None,
            mark_done_id: None,
            list: None,
        };

        // Execute the fixture
        task_list
            .call(ToolCallContext::default(), input1)
            .await
            .unwrap();
        let result = task_list
            .call(ToolCallContext::default(), input2)
            .await
            .unwrap();

        insta::assert_snapshot!(result);
    }

    #[tokio::test]
    async fn test_pop_front() {
        // Create fixture
        let task_list = create_task_list();

        // Add a task and mark it as in-progress
        let input1 = TaskListInput {
            append_task: Some("Task to pop".to_string()),
            prepend_task: None,
            pop_front: None,
            pop_back: None,
            mark_done_id: None,
            list: None,
        };
        let input2 = TaskListInput {
            append_task: None,
            prepend_task: None,
            pop_front: Some(true),
            pop_back: None,
            mark_done_id: None,
            list: None,
        };

        // Execute the fixture
        task_list
            .call(ToolCallContext::default(), input1)
            .await
            .unwrap();
        let result = task_list
            .call(ToolCallContext::default(), input2)
            .await
            .unwrap();

        insta::assert_snapshot!(result);
    }

    #[tokio::test]
    async fn test_pop_back() {
        // Create fixture
        let task_list = create_task_list();

        // Add two tasks and mark the last one as in-progress
        let input1 = TaskListInput {
            append_task: Some("Task 1".to_string()),
            prepend_task: None,
            pop_front: None,
            pop_back: None,
            mark_done_id: None,
            list: None,
        };
        let input2 = TaskListInput {
            append_task: Some("Task 2".to_string()),
            prepend_task: None,
            pop_front: None,
            pop_back: None,
            mark_done_id: None,
            list: None,
        };
        let input3 = TaskListInput {
            append_task: None,
            prepend_task: None,
            pop_front: None,
            pop_back: Some(true),
            mark_done_id: None,
            list: None,
        };

        // Execute the fixture
        task_list
            .call(ToolCallContext::default(), input1)
            .await
            .unwrap();
        task_list
            .call(ToolCallContext::default(), input2)
            .await
            .unwrap();
        let result = task_list
            .call(ToolCallContext::default(), input3)
            .await
            .unwrap();

        insta::assert_snapshot!(result);
    }

    #[tokio::test]
    async fn test_mark_done() {
        // Create fixture
        let task_list = create_task_list();

        // Add two tasks, mark first as done
        let input1 = TaskListInput {
            append_task: Some("Task 1".to_string()),
            prepend_task: None,
            pop_front: None,
            pop_back: None,
            mark_done_id: None,
            list: None,
        };

        // Execute the fixture
        task_list
            .call(ToolCallContext::default(), input1)
            .await
            .unwrap();

        // For testing purposes, we'll use ID 1 for the first task
        let id = 1;
        let input3 = TaskListInput {
            append_task: None,
            prepend_task: None,
            pop_front: None,
            pop_back: None,
            mark_done_id: Some(id),
            list: None,
        };
        let result = task_list
            .call(ToolCallContext::default(), input3)
            .await
            .unwrap();

        insta::assert_snapshot!(result);
    }

    #[tokio::test]
    async fn test_empty_list() {
        // Create fixture
        let task_list = create_task_list();
        let input = TaskListInput {
            append_task: None,
            prepend_task: None,
            pop_front: None,
            pop_back: None,
            mark_done_id: None,
            list: Some(true),
        };

        // Execute the fixture
        let actual = task_list
            .call(ToolCallContext::default(), input)
            .await
            .unwrap();

        insta::assert_snapshot!(actual);
    }
}
