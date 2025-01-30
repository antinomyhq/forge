use futures::Stream;
use tokio::sync::mpsc::Receiver;

/// A wrapper around a tokio mpsc::Receiver that implements Stream and provides
/// a callback mechanism for when the stream is dropped.
///
/// This stream automatically closes the underlying receiver when dropped and
/// executes an optional closure, making it useful for cleanup operations.
pub struct ChatReceiverStream<T> {
    receiver: Receiver<T>,
    on_close: Option<Box<dyn FnOnce() + Send>>,
}

impl<T> Drop for ChatReceiverStream<T> {
    fn drop(&mut self) {
        // Execute the on_close callback if present
        if let Some(f) = self.on_close.take() {
            f();
        }
        // Close the receiver to prevent any new messages
        self.receiver.close();
    }
}

impl<T> ChatReceiverStream<T> {
    /// Creates a new ChatReceiverStream from a tokio mpsc::Receiver.
    ///
    /// # Arguments
    /// * `receiver` - The tokio mpsc::Receiver to wrap
    ///
    /// # Returns
    /// A new ChatReceiverStream instance
    pub fn new(receiver: Receiver<T>) -> Self {
        Self { receiver, on_close: None }
    }

    /// Sets a callback function to be executed when the stream is dropped.
    ///
    /// # Arguments
    /// * `f` - A closure that implements FnOnce and is Send
    ///
    /// # Returns
    /// Self with the callback configured
    ///
    /// # Example
    /// ```
    /// let (tx, rx) = tokio::sync::mpsc::channel(1);
    /// let stream = ChatReceiverStream::new(rx)
    ///     .on_close(|| println!("Stream closed!"));
    /// ```
    pub fn on_close<F>(mut self, f: F) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        self.on_close = Some(Box::new(f));
        self
    }
}

impl<T> Stream for ChatReceiverStream<T> {
    type Item = T;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}

#[cfg(test)]
mod test {
    use futures::StreamExt;
    use tokio::sync::{mpsc, oneshot};

    use super::*;

    #[tokio::test]
    async fn test_stream_receives_messages() {
        let (tx, rx) = mpsc::channel(1);
        let mut stream = ChatReceiverStream::new(rx);

        tx.send("test message").await.unwrap();
        let result = stream.next().await;
        assert_eq!(result, Some("test message"));
    }

    #[tokio::test]
    async fn test_on_close_callback() {
        let (_tx, rx) = mpsc::channel::<String>(1);
        let (flag_tx, flag_rx) = oneshot::channel::<()>();

        let stream = ChatReceiverStream::new(rx).on_close(move || {
            flag_tx.send(()).unwrap();
        });

        drop(stream);

        assert_eq!(flag_rx.await, Ok(()));
    }
}
