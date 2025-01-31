use std::future::Future;

use futures::Stream;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;

pub struct MpscStream<T> {
    join_handle: JoinHandle<()>,
    receiver: Receiver<T>,
}

impl<T> MpscStream<T> {
    pub fn spawn<F, S>(f: F) -> MpscStream<T>
    where
        F: (FnOnce(Sender<T>) -> S) + Send + 'static,
        S: Future<Output = ()> + Send + 'static,
    {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        MpscStream { join_handle: tokio::spawn(f(tx)), receiver: rx }
    }
}

impl<T> Stream for MpscStream<T> {
    type Item = T;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}

impl<T> Drop for MpscStream<T> {
    fn drop(&mut self) {
        // Close the receiver to prevent any new messages
        self.receiver.close();
        self.join_handle.abort();
    }
}

#[cfg(test)]
mod test {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use futures::StreamExt;
    use tokio::time::pause;

    use super::*;

    #[tokio::test]
    async fn test_stream_receives_messages() {
        let mut stream = MpscStream::spawn(|tx| async move {
            tx.send("test message").await.unwrap();
        });

        let result = stream.next().await;
        assert_eq!(result, Some("test message"));
    }

    #[tokio::test]
    async fn test_drop_aborts_task() {
        // Pause time to control it manually
        pause();

        let completed = Arc::new(AtomicBool::new(false));
        let completed_clone = completed.clone();

        let stream = MpscStream::spawn(|tx| async move {
            // Try to send a message
            let send_result = tx.send(1).await;
            assert!(send_result.is_ok(), "First send should succeed");

            // Simulate long running task with virtual time
            tokio::time::sleep(Duration::from_secs(1)).await;

            // This should never execute because we'll drop the stream
            completed_clone.store(true, Ordering::SeqCst);

            // This send should fail since receiver is dropped
            let _ = tx.send(2).await;
        });

        // Advance time a small amount to allow first message to be processed
        tokio::time::advance(Duration::from_millis(10)).await;

        // Drop the stream - this should abort the task
        drop(stream);

        // Advance time past when the task would have completed
        tokio::time::advance(Duration::from_secs(2)).await;

        // Verify the task was aborted and didn't complete
        assert!(
            !completed.load(Ordering::SeqCst),
            "Task should have been aborted"
        );
    }
}
