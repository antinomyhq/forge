use serde::{Deserialize, Serialize};

/// Information about an attempt completion in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptCompletionInfo {
    /// The result content from the attempt completion (markdown formatted)
    pub result: String,
}

/// Represents one user message + attempt_completion cycle in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionEntry {
    /// The first user message before this completion
    pub user_message: String,
    /// Information about the attempt completion
    pub completion: AttemptCompletionInfo,
    /// Number of tool calls between the user message and this completion
    pub tool_call_count: usize,
}

/// A summary of a conversation's state showing all completion cycles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    /// All user message + completion pairs in chronological order
    pub entries: Vec<CompletionEntry>,
}
