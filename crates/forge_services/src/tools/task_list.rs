use std::sync::Arc;

use anyhow::Result;
use forge_app::TaskService;
use forge_display::TitleFormat;
use forge_domain::{
    ExecutableTool, NamedTool, Operation, OperationType, Stats, TaskId, TaskListResult,
    ToolCallContext, ToolDescription, ToolName, ToolOutput,
};
use forge_tool_macros::ToolDescription;

use crate::Infrastructure;

/// A powerful task management system designed for handling complex, multi-step
/// workflows. Use this tool when planning or executing tasks that require
/// structured organization, especially for complex projects with multiple
/// steps. Ideal for:
/// 1) Breaking down large problems into manageable subtasks
/// 2) Creating and managing step-by-step action plans
/// 3) Tracking progress across interconnected work items
/// 4) Ensuring sequential completion of dependent tasks
///
/// The tool maintains an ordered task list with status tracking (pending,
/// in-progress, complete), provides statistics on overall progress, and
/// automatically identifies the next task to tackle. Choose this tool whenever
/// you need to organize work that's too complex for a simple checklist
/// or when systematic tracking of completion status is required.
#[derive(Debug, ToolDescription)]
pub struct TaskList<F> {
    infra: Arc<F>,
}

impl<F: Infrastructure> TaskList<F> {
    /// Creates a new TaskList tool with the given infrastructure.
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }

    /// Calculate statistics for the current task list.
    async fn calculate_stats(&self) -> Result<Stats> {
        let (total_tasks, done_tasks, pending_tasks, in_progress_tasks) =
            self.infra.task_service().stats().await?;

        Ok(Stats { total_tasks, done_tasks, pending_tasks, in_progress_tasks })
    }

    /// Handle the append operation, adding a task to the end of the list.
    async fn append(&self, descriptions: Vec<String>) -> Result<TaskListResult> {
        self.infra.task_service().append(descriptions).await?;

        let stats = self.calculate_stats().await?;

        Ok(TaskListResult {
            message: Some("Task added to the end of the list.".to_string()),
            next_task: None,
            stats,
            tasks: None,
        })
    }

    /// Handle the prepend operation, adding a task to the beginning of the
    /// list.
    async fn prepend(&self, descriptions: Vec<String>) -> Result<TaskListResult> {
        self.infra.task_service().prepend(descriptions).await?;

        let stats = self.calculate_stats().await?;

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
        let task_option = self.infra.task_service().pop_front().await?;

        let stats = self.calculate_stats().await?;

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
    async fn mark_done(&self, id: TaskId) -> Result<TaskListResult> {
        let found_task = self.infra.task_service().mark_done(id.clone()).await?;

        if found_task.is_none() {
            return Ok(TaskListResult {
                message: Some(format!("No task found with ID {id}.")),
                next_task: None,
                stats: self.calculate_stats().await?,
                tasks: None,
            });
        }

        let next_task = self.infra.task_service().find_next_pending().await?;
        let stats = self.calculate_stats().await?;

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

    /// Handle the list operation, returning all tasks in markdown format.
    async fn list(&self) -> Result<TaskListResult> {
        let tasks = self.infra.task_service().list().await?;
        let stats = self.calculate_stats().await?;
        let markdown = self.infra.task_service().format_markdown().await?;

        // Use TaskDisplayService for consistent formatting

        Ok(TaskListResult {
            message: Some(markdown),
            next_task: None,
            stats,
            tasks: Some(tasks),
        })
    }
}

impl<F> NamedTool for TaskList<F> {
    fn tool_name() -> ToolName {
        ToolName::new("forge_tool_task_list")
    }
}

fn format_input(input: &Operation) -> String {
    match input.operation_type {
        OperationType::Append => {
            format!("Append: {}", input.descriptions.join(", ").as_str())
        }
        OperationType::Prepend => {
            format!("Prepend: {}", input.descriptions.join(", ").as_str())
        }
        OperationType::Next => "Next".to_string(),
        OperationType::Done => {
            format!("Done: {}", input.task_id.as_ref().unwrap_or(&TaskId::new()))
        }
        OperationType::List => "List".to_string(),
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> ExecutableTool for TaskList<F> {
    type Input = Operation;

    async fn call(&self, context: &mut ToolCallContext, input: Self::Input) -> Result<ToolOutput> {
        context
            .send_text(TitleFormat::debug("TaskList").sub_title(format_input(&input)))
            .await?;

        let result = match input.operation_type {
            OperationType::Append => self.append(input.descriptions).await?,
            OperationType::Prepend => self.prepend(input.descriptions).await?,
            OperationType::Next => self.pop_front().await?,
            OperationType::Done => {
                let task_id = input
                    .task_id
                    .ok_or_else(|| anyhow::anyhow!("Task ID required for done operation"))?;
                self.mark_done(task_id).await?
            }
            OperationType::List => self.list().await?,
        };

        Ok(ToolOutput::text(result.to_string()))
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

        let input = Operation::append(vec!["Test task".to_string()]);

        // Execute the fixture
        let result = task_list
            .call(&mut ToolCallContext::default(), input)
            .await
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        insta::assert_snapshot!(result);
    }

    #[tokio::test]
    async fn test_prepend_task() {
        // Create fixture
        let task_list = create_task_list();

        let input1 = Operation::append(vec!["Task 1".to_string()]);
        let input2 = Operation::prepend(vec!["Task 2".to_string()]);

        // Execute the fixture
        task_list
            .call(&mut ToolCallContext::default(), input1)
            .await
            .unwrap();
        let result = task_list
            .call(&mut ToolCallContext::default(), input2)
            .await
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        insta::assert_snapshot!(result);
    }

    #[tokio::test]
    async fn test_pop_front() {
        // Create fixture
        let task_list = create_task_list();

        let input1 = Operation::append(vec!["Task to pop".to_string()]);
        let input2 = Operation::next();

        // Execute the fixture
        task_list
            .call(&mut ToolCallContext::default(), input1)
            .await
            .unwrap();
        let result = task_list
            .call(&mut ToolCallContext::default(), input2)
            .await
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        insta::assert_snapshot!(result);
    }

    #[tokio::test]
    async fn test_mark_done() {
        // Create fixture
        let task_list = create_task_list();
        let input1 = Operation::append(vec!["Task to mark done".to_string()]);

        // Execute the fixture - first add a task
        task_list
            .call(&mut ToolCallContext::default(), input1)
            .await
            .unwrap();

        // Get the task ID from the service (we know it will be deterministic)
        let tasks = task_list.infra.task_service().list().await.unwrap();
        let task_id = tasks[0].id.clone();

        // Mark the task as done
        let input2 = Operation::done(task_id);
        let result = task_list
            .call(&mut ToolCallContext::default(), input2)
            .await
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        insta::assert_snapshot!(result);
    }

    #[tokio::test]
    async fn test_empty_list() {
        // Create fixture
        let task_list = create_task_list();
        let input = Operation::next();

        // Execute the fixture
        let actual = task_list
            .call(&mut ToolCallContext::default(), input)
            .await
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        insta::assert_snapshot!(actual);
    }

    #[tokio::test]
    async fn test_list_operation() {
        // Create fixture
        let task_list = create_task_list();

        // Add some tasks first
        let input1 = Operation::append(vec!["Task 1".to_string(), "Task 2".to_string()]);
        let input2 = Operation::list();

        // Execute the fixture
        task_list
            .call(&mut ToolCallContext::default(), input1)
            .await
            .unwrap();

        let result = task_list
            .call(&mut ToolCallContext::default(), input2)
            .await
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        insta::assert_snapshot!(result);
    }
}
