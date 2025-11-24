use thiserror::Error;

/// Errors that can occur during inline shell command processing
#[derive(Debug, Error)]
pub enum InlineShellError {
    /// Empty command found in inline shell syntax
    #[error("Empty command found in inline shell syntax at position {position}")]
    EmptyCommand {
        /// Position where the empty command was found
        position: usize,
    },

    /// Malformed inline shell syntax
    #[error("Malformed inline shell syntax at position {position}: {reason}")]
    MalformedSyntax {
        /// Position where the malformed syntax was found
        position: usize,
        /// Description of the syntax issue
        reason: String,
    },

    /// Command execution failed
    #[error("Command execution failed for '{command}': {source}")]
    ExecutionFailed {
        /// The command that failed
        command: String,
        /// The underlying error
        #[source]
        source: anyhow::Error,
    },

    /// Too many commands found in content
    #[error(
        "Too many inline shell commands found ({count}). Maximum allowed is {max_allowed}. Configure with FORGE_INLINE_MAX_COMMANDS environment variable"
    )]
    TooManyCommands {
        /// Number of commands found
        count: usize,
        /// Maximum allowed commands
        max_allowed: usize,
    },

    /// Command execution timeout
    #[error(
        "Command execution timed out for '{command}' after {timeout_seconds} seconds. Configure with FORGE_INLINE_COMMAND_TIMEOUT environment variable"
    )]
    ExecutionTimeout {
        /// The command that timed out
        command: String,
        /// Timeout duration in seconds
        timeout_seconds: u64,
    },

    /// Command output exceeds maximum allowed length
    #[error(
        "Command output for '{command}' exceeds maximum allowed length of {max_length} characters (actual: {actual_length}). Configure with FORGE_INLINE_MAX_OUTPUT_LENGTH environment variable"
    )]
    OutputTooLarge {
        /// The command that produced too much output
        command: String,
        /// Maximum allowed output length
        max_length: usize,
        /// Actual output length
        actual_length: usize,
    },

    /// Command blocked in restricted mode
    #[error("Command '{command}' blocked in restricted mode")]
    RestrictedModeBlocked {
        /// The command that was blocked
        command: String,
    },

    /// Command blocked by policy check
    #[error("Command '{command}' blocked by policy")]
    PolicyBlocked {
        /// The command that was blocked
        command: String,
    },

    /// Policy check failed for command
    #[error("Policy check failed for command '{command}': {source}")]
    PolicyCheckFailed {
        /// The command that failed policy check
        command: String,
        /// The underlying error
        #[source]
        source: anyhow::Error,
    },
}
