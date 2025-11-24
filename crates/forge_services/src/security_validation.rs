use anyhow::Result;
use forge_domain::inline_shell::{
    SecurityCheckResult, analyze_content_security, check_command_security,
};

/// Service for validating security of shell commands
pub struct SecurityValidationService;

impl SecurityValidationService {
    /// Create a new security validation service
    pub fn new() -> Self {
        Self
    }
}

impl Default for SecurityValidationService {
    fn default() -> Self {
        Self::new()
    }
}

/// Security validation operations
impl SecurityValidationService {
    /// Check if a command is dangerous in restricted mode
    pub fn is_command_blocked(&self, command: &str, restricted: bool) -> Result<bool> {
        if !restricted {
            return Ok(false);
        }

        let security_result = check_command_security(command);
        Ok(security_result.is_dangerous)
    }

    /// Get security analysis for a command
    pub fn analyze_command(&self, command: &str) -> Result<SecurityCheckResult> {
        Ok(check_command_security(command))
    }

    /// Analyze security of content with inline shell commands
    pub fn analyze_content(&self, content: &str) -> Result<Vec<SecurityCheckResult>> {
        Ok(analyze_content_security(content))
    }

    /// Get blocked command reason
    pub fn get_block_reason(&self, command: &str) -> Result<String> {
        let security_result = check_command_security(command);
        if security_result.is_dangerous {
            Ok(format!(
                "Command blocked in restricted mode: {} (Severity: {:?})",
                security_result.reason, security_result.severity
            ))
        } else {
            Ok("Command is not blocked".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::inline_shell::SecuritySeverity;

    use super::*;

    #[test]
    fn test_is_command_blocked_restricted_mode() {
        let service = SecurityValidationService::new();

        // Safe command should not be blocked
        assert!(!service.is_command_blocked("echo hello", true).unwrap());

        // Dangerous command should be blocked
        assert!(service.is_command_blocked("rm -rf /", true).unwrap());
    }

    #[test]
    fn test_is_command_blocked_unrestricted_mode() {
        let service = SecurityValidationService::new();

        // All commands should be allowed in unrestricted mode
        assert!(!service.is_command_blocked("echo hello", false).unwrap());
        assert!(!service.is_command_blocked("rm -rf /", false).unwrap());
    }

    #[test]
    fn test_analyze_command() {
        let service = SecurityValidationService::new();
        let result = service.analyze_command("rm -rf /").unwrap();

        assert!(result.is_dangerous);
        assert_eq!(result.severity, SecuritySeverity::Critical);
        assert!(result.reason.contains("filesystem"));
    }

    #[test]
    fn test_analyze_content() {
        let service = SecurityValidationService::new();
        let content = "Safe text ![echo hello] dangerous ![rm -rf /] end";
        let results = service.analyze_content(content).unwrap();

        assert_eq!(results.len(), 2);
        assert!(!results[0].is_dangerous); // echo hello
        assert!(results[1].is_dangerous); // rm -rf /
    }

    #[test]
    fn test_get_block_reason() {
        let service = SecurityValidationService::new();

        // Dangerous command reason
        let reason = service.get_block_reason("rm -rf /").unwrap();
        assert!(reason.contains("blocked"));
        assert!(reason.contains("Critical"));

        // Safe command reason
        let reason = service.get_block_reason("echo hello").unwrap();
        assert_eq!(reason, "Command is not blocked");
    }
}
