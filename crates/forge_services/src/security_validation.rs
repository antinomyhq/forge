use anyhow::Result;
use forge_domain::inline_shell::ports::SecurityAnalysis;
use forge_domain::inline_shell::{
    SecurityCheckResult, SecuritySeverity, analyze_content_security, check_command_security,
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
    ///
    /// # Arguments
    /// * `command` - The command to check
    /// * `restricted` - Whether we're in restricted mode
    ///
    /// # Returns
    /// * `Result<bool>` - Ok(true) if command is blocked, Ok(false) if allowed
    ///
    /// # Errors
    /// Returns error if security analysis fails
    pub fn is_command_blocked(&self, command: &str, restricted: bool) -> Result<bool> {
        if !restricted {
            return Ok(false);
        }

        let security_result = check_command_security(command);
        Ok(security_result.is_dangerous)
    }

    /// Get security analysis for a command
    ///
    /// # Arguments
    /// * `command` - The command to analyze
    ///
    /// # Returns
    /// * `Result<SecurityCheckResult>` - Security analysis result
    pub fn analyze_command(&self, command: &str) -> Result<SecurityCheckResult> {
        Ok(check_command_security(command))
    }

    /// Analyze security of content with inline shell commands
    ///
    /// # Arguments
    /// * `content` - The content to analyze
    ///
    /// # Returns
    /// * `Result<Vec<SecurityCheckResult>>` - Security results for dangerous
    ///   commands
    pub fn analyze_content(&self, content: &str) -> Result<Vec<SecurityCheckResult>> {
        Ok(analyze_content_security(content))
    }

    /// Get blocked command reason
    ///
    /// # Arguments
    /// * `command` - The command to get reason for
    ///
    /// # Returns
    /// * `Result<String>` - Reason why command is blocked
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

    /// Check if command severity exceeds threshold
    ///
    /// # Arguments
    /// * `command` - The command to check
    /// * `max_severity` - Maximum allowed severity level
    ///
    /// # Returns
    /// * `Result<bool>` - Ok(true) if command exceeds threshold
    pub fn exceeds_severity_threshold(
        &self,
        command: &str,
        max_severity: SecuritySeverity,
    ) -> Result<bool> {
        let security_result = check_command_security(command);
        Ok(security_result.severity > max_severity)
    }

    /// Get all dangerous commands in content above severity threshold
    ///
    /// # Arguments
    /// * `content` - Content to analyze
    /// * `min_severity` - Minimum severity to include in results
    ///
    /// # Returns
    /// * `Result<Vec<SecurityCheckResult>>` - Commands meeting severity
    ///   threshold
    pub fn get_dangerous_commands_by_severity(
        &self,
        content: &str,
        min_severity: SecuritySeverity,
    ) -> Result<Vec<SecurityCheckResult>> {
        let all_dangerous = analyze_content_security(content);
        Ok(all_dangerous
            .into_iter()
            .filter(|result| result.severity >= min_severity)
            .collect())
    }
}

/// Implementation of SecurityAnalysis port for SecurityValidationService
impl SecurityAnalysis for SecurityValidationService {
    /// Check if a command is potentially dangerous and return detailed security
    /// analysis
    fn check_command_security(
        &self,
        command: &str,
    ) -> forge_domain::inline_shell::SecurityCheckResult {
        check_command_security(command)
    }

    /// Check if a command is potentially dangerous (legacy function for
    /// backward compatibility)
    fn is_dangerous_command(&self, command: &str) -> bool {
        check_command_security(command).is_dangerous
    }

    /// Get all dangerous commands found in content with their security analysis
    fn analyze_content_security(
        &self,
        content: &str,
    ) -> Vec<forge_domain::inline_shell::SecurityCheckResult> {
        analyze_content_security(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_security_service() -> SecurityValidationService {
        SecurityValidationService::new()
    }

    #[test]
    fn test_is_command_blocked_restricted_mode() {
        let service = fixture_security_service();

        // Dangerous command should be blocked in restricted mode
        assert!(service.is_command_blocked("rm -rf /", true).unwrap());

        // Safe command should not be blocked
        assert!(!service.is_command_blocked("echo hello", true).unwrap());
    }

    #[test]
    fn test_is_command_blocked_unrestricted_mode() {
        let service = fixture_security_service();

        // Nothing should be blocked in unrestricted mode
        assert!(!service.is_command_blocked("rm -rf /", false).unwrap());
        assert!(!service.is_command_blocked("echo hello", false).unwrap());
    }

    #[test]
    fn test_analyze_command() {
        let service = fixture_security_service();

        let result = service.analyze_command("rm -rf /").unwrap();
        assert!(result.is_dangerous);
        assert_eq!(result.severity, SecuritySeverity::Critical);
        assert!(result.reason.contains("filesystem"));
    }

    #[test]
    fn test_analyze_content() {
        let service = fixture_security_service();

        let content = "Run ![ls -la] and ![rm -rf /] and ![echo hello]";
        let results = service.analyze_content(content).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].severity, SecuritySeverity::Critical);
    }

    #[test]
    fn test_get_block_reason() {
        let service = fixture_security_service();

        let reason = service.get_block_reason("rm -rf /").unwrap();
        assert!(reason.contains("Command blocked in restricted mode"));
        assert!(reason.contains("Critical"));

        let safe_reason = service.get_block_reason("echo hello").unwrap();
        assert_eq!(safe_reason, "Command is not blocked");
    }

    #[test]
    fn test_exceeds_severity_threshold() {
        let service = fixture_security_service();

        // Critical command exceeds medium threshold
        assert!(
            service
                .exceeds_severity_threshold("rm -rf /", SecuritySeverity::Medium)
                .unwrap()
        );

        // Low severity command doesn't exceed medium threshold
        assert!(
            !service
                .exceeds_severity_threshold("su root", SecuritySeverity::Medium)
                .unwrap()
        );
    }

    #[test]
    fn test_get_dangerous_commands_by_severity() {
        let service = fixture_security_service();

        let content = "Run ![su root] and ![chmod 777 file] and ![rm -rf /]";

        // Only high and critical severity commands
        let high_and_above = service
            .get_dangerous_commands_by_severity(content, SecuritySeverity::High)
            .unwrap();
        assert_eq!(high_and_above.len(), 2); // chmod 777 (High) and rm -rf / (Critical)

        // Only critical severity commands
        let critical_only = service
            .get_dangerous_commands_by_severity(content, SecuritySeverity::Critical)
            .unwrap();
        assert_eq!(critical_only.len(), 1); // rm -rf / only
    }
}
