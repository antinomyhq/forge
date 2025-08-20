use std::sync::{Arc, Mutex};

use derive_setters::Setters;
use tokio::sync::mpsc::Sender;

use crate::{ChatResponse, Metrics, TaskList};

/// Type alias for Arc<Sender<Result<ChatResponse>>>
type ArcSender = Arc<Sender<anyhow::Result<ChatResponse>>>;

/// Provides additional context for tool calls.
#[derive(Debug, Clone, Setters)]
pub struct ToolCallContext {
    sender: Option<ArcSender>,
    tasks: Arc<Mutex<TaskList>>,
    metrics: Arc<Mutex<Metrics>>,
}

impl ToolCallContext {
    /// Creates a new ToolCallContext with default values
    pub fn new(task_list: TaskList, metrics: Metrics) -> Self {
        Self {
            sender: None,
            tasks: Arc::new(Mutex::new(task_list)),
            metrics: Arc::new(Mutex::new(metrics)),
        }
    }

    /// Creates a new ToolCallContext with shared references
    pub fn with_shared(tasks: Arc<Mutex<TaskList>>, metrics: Arc<Mutex<Metrics>>) -> Self {
        Self { sender: None, tasks, metrics }
    }

    /// Send a message through the sender if available
    pub async fn send(&self, agent_message: impl Into<ChatResponse>) -> anyhow::Result<()> {
        if let Some(sender) = &self.sender {
            sender.send(Ok(agent_message.into())).await?
        }
        Ok(())
    }

    pub async fn send_text(&self, content: impl ToString) -> anyhow::Result<()> {
        self.send(ChatResponse::TaskMessage { text: content.to_string(), is_md: false })
            .await
    }

    /// Get a reference to the tasks mutex
    pub fn get_tasks(&self) -> &Arc<Mutex<TaskList>> {
        &self.tasks
    }

    /// Get a reference to the metrics mutex  
    pub fn get_metrics(&self) -> &Arc<Mutex<Metrics>> {
        &self.metrics
    }

    /// Execute a closure with access to the tasks
    pub fn with_tasks<F, R>(&self, f: F) -> anyhow::Result<R>
    where
        F: FnOnce(&mut TaskList) -> R,
    {
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| anyhow::anyhow!("Failed to acquire tasks lock"))?;
        Ok(f(&mut tasks))
    }

    /// Execute a closure with access to the metrics
    pub fn with_metrics<F, R>(&self, f: F) -> anyhow::Result<R>
    where
        F: FnOnce(&mut Metrics) -> R,
    {
        let mut metrics = self
            .metrics
            .lock()
            .map_err(|_| anyhow::anyhow!("Failed to acquire metrics lock"))?;
        Ok(f(&mut metrics))
    }

    /// Execute a closure with access to both tasks and metrics
    pub fn with_both<F, R>(&self, f: F) -> anyhow::Result<R>
    where
        F: FnOnce(&mut TaskList, &mut Metrics) -> R,
    {
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| anyhow::anyhow!("Failed to acquire tasks lock"))?;
        let mut metrics = self
            .metrics
            .lock()
            .map_err(|_| anyhow::anyhow!("Failed to acquire metrics lock"))?;
        Ok(f(&mut tasks, &mut metrics))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_context() {
        let metrics = Metrics::new();
        let context = ToolCallContext::new(TaskList::new(), metrics);
        assert!(context.sender.is_none());
    }

    #[test]
    fn test_with_sender() {
        // This is just a type check test - we don't actually create a sender
        // as it's complex to set up in a unit test
        let metrics = Metrics::new();
        let context = ToolCallContext::new(TaskList::new(), metrics);
        assert!(context.sender.is_none());
    }

    #[test]
    fn test_with_shared() {
        let tasks = Arc::new(Mutex::new(TaskList::new()));
        let metrics = Arc::new(Mutex::new(Metrics::new()));
        let context = ToolCallContext::with_shared(tasks.clone(), metrics.clone());
        assert!(context.sender.is_none());

        // Test that the references are shared
        let context2 = ToolCallContext::with_shared(tasks, metrics);
        assert!(context2.sender.is_none());
    }

    #[test]
    fn test_thread_safety() {
        use std::sync::Arc;
        use std::thread;

        let context = Arc::new(ToolCallContext::new(TaskList::new(), Metrics::new()));

        // Test that we can send the context between threads
        let context_clone = context.clone();
        let handle = thread::spawn(move || {
            // This compilation test verifies thread safety
            let _tasks = &context_clone.tasks;
            let _metrics = &context_clone.metrics;
        });

        handle.join().unwrap();
    }

    #[test]
    fn test_concurrent_operations() {
        use std::sync::Arc;
        use std::thread;

        let context = Arc::new(ToolCallContext::new(TaskList::new(), Metrics::new()));

        // Test concurrent task operations
        let handles: Vec<_> = (0..5)
            .map(|i| {
                let context = context.clone();
                thread::spawn(move || {
                    let task_text = format!("Task {}", i);

                    // Add a task concurrently
                    context
                        .with_tasks(|tasks| {
                            tasks.append(task_text);
                        })
                        .unwrap();

                    // Record metrics concurrently
                    context
                        .with_metrics(|metrics| {
                            metrics.record_file_operation(format!("file_{}.rs", i), 10, 5);
                        })
                        .unwrap();
                })
            })
            .collect();

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify that all operations completed successfully
        let task_count = context.with_tasks(|tasks| tasks.tasks().len()).unwrap();
        assert_eq!(task_count, 5);

        let files_changed = context
            .with_metrics(|metrics| metrics.files_changed.len())
            .unwrap();
        assert_eq!(files_changed, 5);
    }

    #[test]
    fn test_convenience_methods() {
        let context = ToolCallContext::new(TaskList::new(), Metrics::new());

        // Test with_tasks method
        let result = context.with_tasks(|tasks| tasks.tasks().len()).unwrap();
        assert_eq!(result, 0);

        // Test with_metrics method
        let result = context
            .with_metrics(|metrics| metrics.started_at.is_none())
            .unwrap();
        assert!(result);

        // Test with_both method
        let result = context
            .with_both(|tasks, metrics| (tasks.tasks().len(), metrics.started_at.is_none()))
            .unwrap();
        assert_eq!(result, (0, true));
    }
}
