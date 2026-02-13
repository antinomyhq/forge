use forge_domain::InterruptionReason;

/// Formatted interruption message with title and description
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterruptionMessage {
    /// Short title describing the interruption type
    pub title: String,
    /// Detailed description explaining the interruption and potential next steps
    pub description: String,
}

/// Service for formatting interruption reasons into user-friendly messages
///
/// This service provides protocol-agnostic formatting of interruption reasons.
/// Different protocols (ACP, REST, etc.) can use this to generate consistent
/// user-facing messages.
#[derive(Debug, Clone, Copy)]
pub struct InterruptionService;

impl InterruptionService {
    /// Creates a new interruption service
    pub fn new() -> Self {
        Self
    }

    /// Formats an interruption reason into a user-friendly message
    ///
    /// # Arguments
    ///
    /// * `reason` - The interruption reason to format
    ///
    /// # Returns
    ///
    /// An `InterruptionMessage` with a title and description
    pub fn format_interruption(&self, reason: &InterruptionReason) -> InterruptionMessage {
        match reason {
            InterruptionReason::MaxRequestPerTurnLimitReached { limit } => {
                InterruptionMessage {
                    title: "Maximum Request Limit Reached".to_string(),
                    description: format!(
                        "The agent has reached the maximum request limit ({}) for this turn. \
                        This may indicate the agent is stuck in a loop or the task is too complex.",
                        limit
                    ),
                }
            }
            InterruptionReason::MaxToolFailurePerTurnLimitReached { limit, errors } => {
                let error_summary = if errors.is_empty() {
                    String::new()
                } else {
                    let error_list = errors
                        .iter()
                        .map(|(tool, count)| format!("  • {} failed {} time(s)", tool, count))
                        .collect::<Vec<_>>()
                        .join("\n");
                    format!("\n\nFailed tools:\n{}", error_list)
                };

                InterruptionMessage {
                    title: "Maximum Tool Failure Limit Reached".to_string(),
                    description: format!(
                        "The agent has reached the maximum tool failure limit ({}) for this turn. \
                        Continuing may result in more errors or unexpected behavior.{}",
                        limit, error_summary
                    ),
                }
            }
        }
    }
}

impl Default for InterruptionService {
    fn default() -> Self {
        Self::new()
    }
}
