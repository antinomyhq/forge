use derive_setters::Setters;
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

impl Buffer {
    pub fn input<T: ToString>(content: T) -> Self {
        Self { event: BufferEvent::Input, content: content.to_string() }
    }
    pub fn output<T: ToString>(content: T) -> Self {
        Self { event: BufferEvent::Output, content: content.to_string() }
    }
}
