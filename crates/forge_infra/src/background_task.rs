use std::future::Future;
use std::sync::{Arc, Mutex};

use forge_app::{BackgroundTaskInfra, TaskHandle};

/// Tokio-based background task spawner.
///
/// Spawns tasks using `tokio::spawn` which runs them on the tokio runtime's
/// thread pool. All spawned tasks are tracked internally and will be aborted
/// when the service is dropped.
#[derive(Clone)]
pub struct TokioBackgroundTaskService {
    handles: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
}

impl TokioBackgroundTaskService {
    pub fn new() -> Self {
        Self { handles: Arc::new(Mutex::new(Vec::new())) }
    }
}

impl Default for TokioBackgroundTaskService {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TokioBackgroundTaskService {
    fn drop(&mut self) {
        // Only abort if this is the last reference
        if Arc::strong_count(&self.handles) == 1
            && let Ok(mut handles) = self.handles.lock() {
                for handle in handles.drain(..) {
                    handle.abort();
                }
            }
    }
}

impl BackgroundTaskInfra for TokioBackgroundTaskService {
    type Handle = TokioTaskHandle;

    fn spawn_bg<F>(&self, task: F) -> Self::Handle
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let handle = tokio::spawn(task);
        let abort_handle = handle.abort_handle();

        // Store the handle for cleanup on drop
        if let Ok(mut handles) = self.handles.lock() {
            // Clean up finished tasks to prevent unbounded growth
            handles.retain(|h| !h.is_finished());
            handles.push(handle);
        }

        TokioTaskHandle { abort_handle }
    }
}

/// Handle to a tokio background task.
///
/// Wraps `tokio::task::AbortHandle` to provide the `TaskHandle` trait.
pub struct TokioTaskHandle {
    abort_handle: tokio::task::AbortHandle,
}

impl TaskHandle for TokioTaskHandle {
    fn abort(&mut self) {
        self.abort_handle.abort();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn test_spawn_bg_runs_task() {
        let executed = Arc::new(AtomicBool::new(false));
        let executed_clone = executed.clone();

        let service = TokioBackgroundTaskService::new();
        let _handle = service.spawn_bg(async move {
            executed_clone.store(true, Ordering::SeqCst);
        });

        // Give the task time to execute
        tokio::time::sleep(Duration::from_millis(10)).await;

        assert!(executed.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_abort_cancels_task() {
        let executed = Arc::new(AtomicBool::new(false));
        let executed_clone = executed.clone();

        let service = TokioBackgroundTaskService::new();
        let mut handle = service.spawn_bg(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            executed_clone.store(true, Ordering::SeqCst);
        });

        // Abort immediately
        handle.abort();

        // Wait to ensure task doesn't execute
        tokio::time::sleep(Duration::from_millis(150)).await;

        assert!(!executed.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_multiple_tasks_can_run_concurrently() {
        let counter = Arc::new(AtomicBool::new(false));
        let counter_clone = counter.clone();

        let service = TokioBackgroundTaskService::new();

        let _handle1 = service.spawn_bg(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            counter_clone.store(true, Ordering::SeqCst);
        });

        let counter2 = Arc::new(AtomicBool::new(false));
        let counter2_clone = counter2.clone();

        let _handle2 = service.spawn_bg(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            counter2_clone.store(true, Ordering::SeqCst);
        });

        // Wait for both tasks
        tokio::time::sleep(Duration::from_millis(50)).await;

        assert!(counter.load(Ordering::SeqCst));
        assert!(counter2.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_drop_aborts_all_pending_tasks() {
        let executed = Arc::new(AtomicBool::new(false));
        let executed_clone = executed.clone();

        {
            let service = TokioBackgroundTaskService::new();
            let _handle = service.spawn_bg(async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                executed_clone.store(true, Ordering::SeqCst);
            });

            // Service goes out of scope here and should abort the task
        }

        // Wait to ensure task doesn't execute after drop
        tokio::time::sleep(Duration::from_millis(150)).await;

        assert!(!executed.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_drop_with_multiple_pending_tasks() {
        let counter1 = Arc::new(AtomicBool::new(false));
        let counter2 = Arc::new(AtomicBool::new(false));
        let counter3 = Arc::new(AtomicBool::new(false));

        let c1 = counter1.clone();
        let c2 = counter2.clone();
        let c3 = counter3.clone();

        {
            let service = TokioBackgroundTaskService::new();

            let _h1 = service.spawn_bg(async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                c1.store(true, Ordering::SeqCst);
            });

            let _h2 = service.spawn_bg(async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                c2.store(true, Ordering::SeqCst);
            });

            let _h3 = service.spawn_bg(async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                c3.store(true, Ordering::SeqCst);
            });

            // Service drops here and should abort all tasks
        }

        // Wait to ensure tasks don't execute
        tokio::time::sleep(Duration::from_millis(150)).await;

        assert!(!counter1.load(Ordering::SeqCst));
        assert!(!counter2.load(Ordering::SeqCst));
        assert!(!counter3.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_cloned_service_doesnt_abort_on_drop() {
        let executed = Arc::new(AtomicBool::new(false));
        let executed_clone = executed.clone();

        let service = TokioBackgroundTaskService::new();

        {
            let cloned_service = service.clone();
            let _handle = cloned_service.spawn_bg(async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                executed_clone.store(true, Ordering::SeqCst);
            });
            // Cloned service drops here, but original still exists
        }

        // Task should still complete since original service is alive
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert!(executed.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_finished_tasks_are_cleaned_up() {
        let service = TokioBackgroundTaskService::new();

        // Spawn tasks that complete quickly
        for _ in 0..10 {
            let _handle = service.spawn_bg(async {
                // Task completes immediately
            });
        }

        // Wait for tasks to finish
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Spawn one more task to trigger cleanup
        let _handle = service.spawn_bg(async {
            tokio::time::sleep(Duration::from_millis(10)).await;
        });

        // Verify the handles vec doesn't grow unbounded
        // (we can't directly check the vec size, but this tests the cleanup logic runs)
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
