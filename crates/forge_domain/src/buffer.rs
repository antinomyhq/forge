use std::pin::Pin;
use std::task::{Context, Poll};

use derive_setters::Setters;
use futures::Stream;
use pin_project::pin_project;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BufferEvent {
    Input,
    Output,
}

#[derive(Debug, Clone, PartialEq, Setters, Serialize, Deserialize)]
#[setters(into)]
pub struct Buffer {
    pub event: BufferEvent,
    pub content: String,
}

/// Iterator for reading JSONL (JSON Lines) files
/// Each line in the file should contain a valid JSON object
#[pin_project]
pub struct JsonlIterator {
    #[pin]
    stream: Pin<Box<dyn Stream<Item = Buffer> + Send + Sync>>,
}

impl JsonlIterator {
    pub fn new(stream: Pin<Box<dyn Stream<Item = Buffer> + Send + Sync>>) -> Self {
        Self { stream }
    }
}

impl Stream for JsonlIterator {
    type Item = anyhow::Result<Buffer>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        this.stream.poll_next(cx).map(|opt| opt.map(Ok))
    }
}
