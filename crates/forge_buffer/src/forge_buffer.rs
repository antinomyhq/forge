use serde::{Deserialize, Serialize};
use tokio::fs::File;
use tokio::io::{BufReader, Lines};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BufferEvent {
    Input,
    Output,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Buffer {
    pub event: BufferEvent,
    pub content: String,
}

/// Iterator to read JSONL file
pub struct JsonlIterator {
    pub(crate) lines: Lines<BufReader<File>>,
}

#[derive(Debug)]
/// Used to store input and outputs to a JSONL file to be able to restore forge
/// state in the future.
pub struct ForgeBuffer;

impl Default for ForgeBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl ForgeBuffer {
    pub fn new() -> Self {
        Self
    }
}
