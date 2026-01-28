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

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::ToolOutput;

    #[test]
    fn test_exit_text_conversation_id() {
        let conv_id = ConversationId::generate();
        let exit = Exit::text("Hello", conv_id);

        let actual = exit.conversation_id();
        let expected = conv_id;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_exit_text_as_text() {
        let conv_id = ConversationId::generate();
        let exit = Exit::text("Hello world", conv_id);

        let actual = exit.as_text();
        let expected = Some("Hello world");

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_exit_tool_as_text_returns_none() {
        let conv_id = ConversationId::generate();
        let result = ToolResult::new("test_tool").output(Ok(ToolOutput::text("tool output")));
        let exit = Exit::tool(result, conv_id);

        let actual = exit.as_text();
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_exit_tool_as_tool_result() {
        let conv_id = ConversationId::generate();
        let result = ToolResult::new("test_tool").output(Ok(ToolOutput::text("tool output")));
        let exit = Exit::tool(result.clone(), conv_id);

        let actual = exit.as_tool_result();

        assert!(actual.is_some());
        assert_eq!(actual.unwrap().name, result.name);
    }

    #[test]
    fn test_exit_interrupt_is_interrupt() {
        let conv_id = ConversationId::generate();
        let reason = InterruptionReason::MaxRequestPerTurnLimitReached { limit: 10 };
        let exit = Exit::interrupt(reason, conv_id);

        assert!(exit.is_interrupt());
    }

    #[test]
    fn test_exit_text_is_not_interrupt() {
        let conv_id = ConversationId::generate();
        let exit = Exit::text("Hello", conv_id);

        assert!(!exit.is_interrupt());
    }

    #[test]
    fn test_exit_interrupt_as_text() {
        let conv_id = ConversationId::generate();
        let reason = InterruptionReason::MaxRequestPerTurnLimitReached { limit: 10 };
        let exit = Exit::interrupt(reason, conv_id);

        let actual = exit.as_text();
        let expected = None;

        assert_eq!(actual, expected);
    }
}
