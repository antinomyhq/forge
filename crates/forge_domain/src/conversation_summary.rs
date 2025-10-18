use serde::{Deserialize, Serialize};

/// Represents one user message + assistant response cycle in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionEntry {
    /// The first user message before this assistant response
    pub user_message: String,
    /// The content of the last assistant message before the next user message
    pub assistant_content: String,
    /// Number of tool calls between the user message and this assistant
    /// response
    pub tool_call_count: usize,
}

/// A summary of a conversation's state showing all user message + assistant
/// response pairs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    /// All user message + assistant response pairs in chronological order
    pub entries: Vec<CompletionEntry>,
}
