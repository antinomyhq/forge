use crate::{ConversationId, InterruptionReason, ToolResult};

/// The final output of an agent execution.
///
/// Each variant represents a different outcome type:
/// - `Text`: Normal completion with accumulated text output
/// - `Tool`: Hook captured a specific tool's output as the final result
/// - `Interrupt`: Execution was interrupted (max requests, max errors)
#[derive(Debug, Clone)]
pub enum Exit {
    /// Agent produced text output (normal completion or hook exit).
    Text {
        /// The accumulated text/markdown output from assistant messages
        output: String,
        /// Reference to the conversation
        conversation_id: ConversationId,
    },

    /// Hook captured a specific tool's output as the final result.
    Tool {
        /// The captured tool result
        result: ToolResult,
        /// Reference to the conversation
        conversation_id: ConversationId,
    },

    /// Execution was interrupted (max requests, max errors).
    /// Caller may prompt user to continue.
    Interrupt {
        /// The reason for interruption
        reason: InterruptionReason,
        /// Reference to the conversation
        conversation_id: ConversationId,
    },
}

impl Exit {
    /// Returns the conversation ID regardless of exit variant.
    pub fn conversation_id(&self) -> ConversationId {
        match self {
            Exit::Text { conversation_id, .. }
            | Exit::Tool { conversation_id, .. }
            | Exit::Interrupt { conversation_id, .. } => *conversation_id,
        }
    }

    /// Returns the text output if available.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Exit::Text { output, .. } => Some(output),
            Exit::Tool { .. } | Exit::Interrupt { .. } => None,
        }
    }

    pub fn as_interrupt_reason(&self) -> Option<&InterruptionReason> {
        match self {
            Exit::Tool { .. } | Exit::Text {  .. } => None,
            Exit::Interrupt { reason, .. } => Some(reason),
        }
    }

    /// Returns the tool result if this was a tool capture exit.
    pub fn as_tool_result(&self) -> Option<&ToolResult> {
        match self {
            Exit::Tool { result, .. } => Some(result),
            _ => None,
        }
    }

    /// Returns true if this was an interrupt exit.
    pub fn is_interrupt(&self) -> bool {
        matches!(self, Exit::Interrupt { .. })
    }

    /// Creates a text exit with the given output and conversation ID.
    pub fn text(output: impl Into<String>, conversation_id: ConversationId) -> Self {
        Self::Text { output: output.into(), conversation_id }
    }

    /// Creates a tool exit with the given result and conversation ID.
    pub fn tool(result: ToolResult, conversation_id: ConversationId) -> Self {
        Self::Tool { result, conversation_id }
    }

    /// Creates an interrupt exit with the given reason, output, and conversation
    /// ID.
    pub fn interrupt(reason: InterruptionReason, conversation_id: ConversationId) -> Self {
        Self::Interrupt { reason, conversation_id }
    }
}