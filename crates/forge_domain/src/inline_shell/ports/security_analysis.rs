/// Port for security analysis operations
///
/// This trait defines the interface for analyzing security aspects of commands,
/// providing an abstraction over security checking and validation operations.
#[allow(async_fn_in_trait)]
pub trait SecurityAnalysis: Send + Sync {
    /// Check if a command is potentially dangerous and return detailed security
    /// analysis
    fn check_command_security(&self, command: &str) -> crate::inline_shell::SecurityCheckResult;

    /// Check if a command is potentially dangerous (legacy function for
    /// backward compatibility)
    fn is_dangerous_command(&self, command: &str) -> bool;

    /// Get all dangerous commands found in content with their security analysis
    fn analyze_content_security(
        &self,
        content: &str,
    ) -> Vec<crate::inline_shell::SecurityCheckResult>;
}
