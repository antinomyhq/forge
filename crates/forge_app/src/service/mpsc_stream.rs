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
    use futures::StreamExt;

    use super::*;

    #[tokio::test]
    async fn test_stream_receives_messages() {
        let mut stream = MpscStream::spawn(|tx| async move {
            tx.send("test message").await.unwrap();
        });

        let result = stream.next().await;
        assert_eq!(result, Some("test message"));
    }
}
