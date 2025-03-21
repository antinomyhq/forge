use std::path::PathBuf;

/// Represents a shard containing one or more files.
#[derive(Debug)]
pub struct Shard {
    /// The merged content of all files in this shard
    pub content: String,
    /// The estimated token count for this shard
    pub estimated_tokens: usize,
    /// The paths of all files included in this shard
    pub file_paths: Vec<PathBuf>,
}
