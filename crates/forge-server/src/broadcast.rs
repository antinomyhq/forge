use std::convert::Infallible;

use axum::response::sse::Event;
use futures::stream::{self, Stream};
use serde::Serialize;
use tokio::sync::broadcast;

use crate::Result;

// Shared state between HTTP server and CLI
#[derive(Clone)]
pub struct App<T> {
    tx: broadcast::Sender<String>,
    _t: std::marker::PhantomData<T>,
}

impl<T> Default for App<T> {
    fn default() -> Self {
        let (tx, _) = broadcast::channel::<String>(100);
        Self { tx, _t: Default::default() }
    }
}

impl<T: Serialize> App<T> {
    #[allow(unused)]
    pub fn dispatch(&self, event: T) -> Result<usize> {
        let json = serde_json::to_string(&event)?;
        Ok(self.tx.send(json)?)
    }

    pub async fn as_stream(&self) -> impl Stream<Item = std::result::Result<Event, Infallible>> {
        let rx = self.tx.subscribe();

        stream::unfold(rx, |mut rx| async move {
            match rx.recv().await {
                Ok(msg) => {
                    let event = Event::default().data(msg);
                    Some((Ok(event), rx))
                }
                Err(_) => None,
            }
        })
    }
}
