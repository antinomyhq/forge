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

#[cfg(test)]
mod tests {
    use super::*;
    use forge_domain::ToolName;
    use pretty_assertions::assert_eq;
    use std::collections::HashMap;

    #[test]
    fn test_format_max_request_limit() {
        let service = InterruptionService::new();
        let reason = InterruptionReason::MaxRequestPerTurnLimitReached { limit: 10 };

        let actual = service.format_interruption(&reason);

        let expected = InterruptionMessage {
            title: "Maximum Request Limit Reached".to_string(),
            description: "The agent has reached the maximum request limit (10) for this turn. \
                This may indicate the agent is stuck in a loop or the task is too complex."
                .to_string(),
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_max_tool_failure_without_errors() {
        let service = InterruptionService::new();
        let reason = InterruptionReason::MaxToolFailurePerTurnLimitReached {
            limit: 5,
            errors: HashMap::new(),
        };

        let actual = service.format_interruption(&reason);

        let expected = InterruptionMessage {
            title: "Maximum Tool Failure Limit Reached".to_string(),
            description: "The agent has reached the maximum tool failure limit (5) for this turn. \
                Continuing may result in more errors or unexpected behavior."
                .to_string(),
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_max_tool_failure_with_errors() {
        let service = InterruptionService::new();
        let mut errors = HashMap::new();
        errors.insert(ToolName::from("read".to_string()), 3);
        errors.insert(ToolName::from("write".to_string()), 2);

        let reason = InterruptionReason::MaxToolFailurePerTurnLimitReached { limit: 5, errors };

        let actual = service.format_interruption(&reason);

        // The order of errors in the HashMap is non-deterministic, so we check both possibilities
        let has_read_first = actual.description.contains("read failed 3 time(s)\n  • write failed 2 time(s)");
        let has_write_first = actual.description.contains("write failed 2 time(s)\n  • read failed 3 time(s)");

        assert_eq!(actual.title, "Maximum Tool Failure Limit Reached");
        assert!(actual.description.contains("The agent has reached the maximum tool failure limit (5)"));
        assert!(actual.description.contains("Failed tools:"));
        assert!(has_read_first || has_write_first, "Expected error summary to contain both tools");
    }

    #[test]
    fn test_default_creates_service() {
        let service = InterruptionService::default();
        let reason = InterruptionReason::MaxRequestPerTurnLimitReached { limit: 1 };

        let message = service.format_interruption(&reason);

        assert_eq!(message.title, "Maximum Request Limit Reached");
    }
}
