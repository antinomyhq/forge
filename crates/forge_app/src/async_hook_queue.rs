//! Async hook result queue — accumulates results from background
//! `asyncRewake` hooks between conversation turns.
//!
//! The orchestrator calls [`AsyncHookResultQueue::drain`] before each
//! `chat()` turn and injects every pending result as a `<system_reminder>`
//! context message.  This mirrors Claude Code's `enqueuePendingNotification`
//! pipeline (`hooks.ts:205-244`).

use std::collections::VecDeque;
use std::sync::Arc;

use forge_domain::PendingHookResult;
use tokio::sync::Mutex;

/// Maximum number of pending results before the oldest entry is dropped.
/// Prevents unbounded growth when hooks fire faster than the orchestrator
/// drains.
const MAX_PENDING: usize = 100;

/// Thread-safe FIFO queue for async hook results.
///
/// Cheap to clone — the inner state lives behind an `Arc<Mutex<_>>`.
#[derive(Debug, Clone, Default)]
pub struct AsyncHookResultQueue {
    inner: Arc<Mutex<VecDeque<PendingHookResult>>>,
}

impl AsyncHookResultQueue {
    /// Create an empty queue.
    pub fn new() -> Self {
        Self { inner: Arc::new(Mutex::new(VecDeque::new())) }
    }

    /// Push a result onto the back of the queue.
    ///
    /// If the queue is already at capacity ([`MAX_PENDING`]), the oldest
    /// entry is silently dropped.
    pub async fn push(&self, result: PendingHookResult) {
        let mut queue = self.inner.lock().await;
        if queue.len() >= MAX_PENDING {
            queue.pop_front(); // drop oldest
        }
        queue.push_back(result);
    }

    /// Drain all pending results and return them in FIFO order.
    pub async fn drain(&self) -> Vec<PendingHookResult> {
        let mut queue = self.inner.lock().await;
        queue.drain(..).collect()
    }

    /// Returns `true` if the queue contains no pending results.
    pub async fn is_empty(&self) -> bool {
        self.inner.lock().await.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_push_and_drain_basic() {
        let queue = AsyncHookResultQueue::new();
        assert!(queue.is_empty().await);

        queue
            .push(PendingHookResult {
                hook_name: "hook-a".into(),
                message: "hello".into(),
                is_blocking: false,
            })
            .await;
        assert!(!queue.is_empty().await);

        let results = queue.drain().await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].hook_name, "hook-a");
        assert_eq!(results[0].message, "hello");
        assert!(!results[0].is_blocking);

        // After drain the queue is empty
        assert!(queue.is_empty().await);
        assert!(queue.drain().await.is_empty());
    }

    #[tokio::test]
    async fn test_drain_preserves_fifo_order() {
        let queue = AsyncHookResultQueue::new();
        for i in 0..5 {
            queue
                .push(PendingHookResult {
                    hook_name: format!("hook-{i}"),
                    message: format!("msg-{i}"),
                    is_blocking: i % 2 == 0,
                })
                .await;
        }

        let results = queue.drain().await;
        assert_eq!(results.len(), 5);
        for (i, r) in results.iter().enumerate() {
            assert_eq!(r.hook_name, format!("hook-{i}"));
            assert_eq!(r.message, format!("msg-{i}"));
        }
    }

    #[tokio::test]
    async fn test_cap_at_100_drops_oldest() {
        let queue = AsyncHookResultQueue::new();

        // Push 101 items
        for i in 0..101 {
            queue
                .push(PendingHookResult {
                    hook_name: format!("hook-{i}"),
                    message: format!("msg-{i}"),
                    is_blocking: false,
                })
                .await;
        }

        let results = queue.drain().await;
        // Should have exactly 100 items
        assert_eq!(results.len(), 100);
        // The oldest (hook-0) was dropped; first item is hook-1
        assert_eq!(results[0].hook_name, "hook-1");
        assert_eq!(results[0].message, "msg-1");
        // Last item is hook-100
        assert_eq!(results[99].hook_name, "hook-100");
        assert_eq!(results[99].message, "msg-100");
    }

    #[tokio::test]
    async fn test_clone_shares_state() {
        let queue1 = AsyncHookResultQueue::new();
        let queue2 = queue1.clone();

        queue1
            .push(PendingHookResult {
                hook_name: "from-1".into(),
                message: "m".into(),
                is_blocking: false,
            })
            .await;

        // Drain from clone sees the item pushed through the original
        let results = queue2.drain().await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].hook_name, "from-1");

        // Original is now empty too
        assert!(queue1.is_empty().await);
    }
}
