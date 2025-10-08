use serde::{Deserialize, Serialize};

/// Information about an attempt completion in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptCompletionInfo {
    /// The result content from the attempt completion (markdown formatted)
    pub result: String,
}

/// Information about an interruption in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterruptionInfo {
    /// The reason for the interruption
    pub reason: String,
}

/// A summary of a conversation's current state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    /// The user message that preceded the completion (if any)
    pub user_message: Option<String>,
    /// Information about the last attempt completion (if any)
    pub completion: Option<AttemptCompletionInfo>,
    /// Information about an interruption (if detected)
    pub interruption: Option<InterruptionInfo>,
}
