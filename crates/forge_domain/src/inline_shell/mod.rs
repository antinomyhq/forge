use std::sync::OnceLock;

use regex::Regex;

pub mod executor;
pub mod inline_error;
pub mod parser;
pub mod ports;

pub use executor::{CommandResult, InlineShellExecutor};
pub use inline_error::InlineShellError;
pub use parser::{InlineShellCommand, ParsedContent, parse_inline_commands};
pub use ports::{CommandExecutor, PolicyRepository, SecurityAnalysis};

/// Security severity levels for dangerous commands
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SecuritySeverity {
    /// Low risk - potentially dangerous but commonly used
    Low,
    /// Medium risk - can cause data loss or system changes
    Medium,
    /// High risk - can cause system-wide damage
    High,
    /// Critical risk - can completely compromise the system
    Critical,
}

/// Security detection result for a command
#[derive(Debug, Clone)]
pub struct SecurityCheckResult {
    /// Whether the command is considered dangerous
    pub is_dangerous: bool,
    /// Security severity level
    pub severity: SecuritySeverity,
    /// Reason for the security classification
    pub reason: String,
    /// Matched pattern that triggered the security check
    pub matched_pattern: String,
}

/// Security pattern definition
struct SecurityPattern {
    /// Regex pattern to match
    regex: Regex,
    /// Security severity level
    severity: SecuritySeverity,
    /// Description of why this pattern is dangerous
    reason: &'static str,
}

/// Compiled security patterns with severity levels
static SECURITY_PATTERNS: OnceLock<Vec<SecurityPattern>> = OnceLock::new();

/// Gets the compiled security patterns
fn get_security_patterns() -> &'static Vec<SecurityPattern> {
    SECURITY_PATTERNS.get_or_init(|| {
        vec![
            // Critical severity patterns
            SecurityPattern {
                regex: Regex::new(r"(?i)^\s*rm\s+-rf\s+/+").unwrap(),
                severity: SecuritySeverity::Critical,
                reason: "Can delete entire filesystem",
            },
            SecurityPattern {
                regex: Regex::new(r"(?i)dd\s+if=/dev/zero").unwrap(),
                severity: SecuritySeverity::Critical,
                reason: "Can overwrite entire disk with zeros",
            },
            SecurityPattern {
                regex: Regex::new(r"(?i)mkfs\.").unwrap(),
                severity: SecuritySeverity::Critical,
                reason: "Can format filesystems",
            },
            // High severity patterns
            SecurityPattern {
                regex: Regex::new(r"(?i)^\s*chmod\s+777").unwrap(),
                severity: SecuritySeverity::High,
                reason: "Sets world-writable permissions on files",
            },
            SecurityPattern {
                regex: Regex::new(r"(?i)^\s*chown\s+root").unwrap(),
                severity: SecuritySeverity::High,
                reason: "Can change file ownership to root",
            },
            SecurityPattern {
                regex: Regex::new(r"(?i)^\s*sudo\s+su").unwrap(),
                severity: SecuritySeverity::High,
                reason: "Can escalate to root privileges",
            },
            SecurityPattern {
                regex: Regex::new(r"(?i)^\s*fdisk").unwrap(),
                severity: SecuritySeverity::High,
                reason: "Can modify disk partitions",
            },
            // Medium severity patterns
            SecurityPattern {
                regex: Regex::new(r"(?i)^\s*sudo\s+rm").unwrap(),
                severity: SecuritySeverity::Medium,
                reason: "Can delete system files with elevated privileges",
            },
            SecurityPattern {
                regex: Regex::new(r"(?i)^\s*dd\s+if=").unwrap(),
                severity: SecuritySeverity::Medium,
                reason: "Can directly write to disk devices",
            },
            SecurityPattern {
                regex: Regex::new(r"(?i)^\s*format").unwrap(),
                severity: SecuritySeverity::Medium,
                reason: "Can format storage devices",
            },
            // Low severity patterns
            SecurityPattern {
                regex: Regex::new(r"(?i)^\s*su\s+root").unwrap(),
                severity: SecuritySeverity::Low,
                reason: "Can switch to root user",
            },
            SecurityPattern {
                regex: Regex::new(r"(?i)^\s*shutdown").unwrap(),
                severity: SecuritySeverity::Low,
                reason: "Can shut down the system",
            },
            SecurityPattern {
                regex: Regex::new(r"(?i)^\s*reboot").unwrap(),
                severity: SecuritySeverity::Low,
                reason: "Can reboot the system",
            },
        ]
    })
}

/// Check if a command is potentially dangerous and return detailed security
/// Performs comprehensive security analysis of a shell command
///
/// # Arguments
/// * `command` - The shell command to analyze
///
/// # Returns
/// * `SecurityCheckResult` containing the analysis results
///
/// # Examples
/// ```
/// use forge_domain::inline_shell::check_command_security;
/// use forge_domain::SecuritySeverity;
///
/// let result = check_command_security("rm -rf /");
/// assert!(result.is_dangerous);
/// assert_eq!(result.severity, SecuritySeverity::Critical);
/// ```
pub fn check_command_security(command: &str) -> SecurityCheckResult {
    let patterns = get_security_patterns();

    for pattern in patterns {
        if pattern.regex.is_match(command) {
            return SecurityCheckResult {
                is_dangerous: true,
                severity: pattern.severity.clone(),
                reason: pattern.reason.to_string(),
                matched_pattern: pattern.regex.as_str().to_string(),
            };
        }
    }

    SecurityCheckResult {
        is_dangerous: false,
        severity: SecuritySeverity::Low,
        reason: "Command appears safe".to_string(),
        matched_pattern: String::new(),
    }
}

/// Check if a command is potentially dangerous (legacy function for backward
/// compatibility)
///
/// # Arguments
/// * `command` - The shell command to check
///
/// # Returns
/// * `true` if the command is considered dangerous, `false` otherwise
///
/// # Examples
/// ```
/// use forge_domain::inline_shell::is_dangerous_command;
///
/// assert!(is_dangerous_command("rm -rf /"));
/// assert!(!is_dangerous_command("echo hello"));
/// ```
pub fn is_dangerous_command(command: &str) -> bool {
    check_command_security(command).is_dangerous
}

/// Get all dangerous commands found in content with their security analysis
///
/// # Arguments
/// * `content` - The content to analyze for inline shell commands
///
/// # Returns
/// * `Vec<SecurityCheckResult>` containing analysis results for all dangerous
///   commands found
///
/// # Examples
/// ```
/// use forge_domain::inline_shell::analyze_content_security;
///
/// let results = analyze_content_security("Run ![rm -rf /] and ![echo hello]");
/// assert_eq!(results.len(), 1); // Only dangerous commands are returned
/// assert!(results[0].is_dangerous);
/// ```
pub fn analyze_content_security(content: &str) -> Vec<SecurityCheckResult> {
    let parser::ParsedContent { commands_found, .. } = parser::parse_inline_commands(content)
        .unwrap_or_else(|_| parser::ParsedContent {
            original_content: content.to_string(),
            commands_found: vec![],
        });

    commands_found
        .iter()
        .map(|cmd| check_command_security(&cmd.command))
        .filter(|result| result.is_dangerous)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_critical_severity_detection() {
        let result = check_command_security("rm -rf /");
        assert!(result.is_dangerous);
        assert_eq!(result.severity, SecuritySeverity::Critical);
        assert!(result.reason.contains("filesystem"));
    }

    #[test]
    fn test_high_severity_detection() {
        let result = check_command_security("chmod 777 /etc/passwd");
        assert!(result.is_dangerous);
        assert_eq!(result.severity, SecuritySeverity::High);
        assert!(result.reason.contains("permissions"));
    }

    #[test]
    fn test_medium_severity_detection() {
        let result = check_command_security("sudo rm some_file");
        assert!(result.is_dangerous);
        assert_eq!(result.severity, SecuritySeverity::Medium);
        assert!(result.reason.contains("system files"));
    }

    #[test]
    fn test_low_severity_detection() {
        let result = check_command_security("su root");
        assert!(result.is_dangerous);
        assert_eq!(result.severity, SecuritySeverity::Low);
        assert!(result.reason.contains("root user"));
    }

    #[test]
    fn test_safe_command() {
        let result = check_command_security("ls -la");
        assert!(!result.is_dangerous);
        assert_eq!(result.severity, SecuritySeverity::Low);
        assert!(result.reason.contains("safe"));
    }

    #[test]
    fn test_case_insensitive_matching() {
        let dangerous_commands = [
            "RM -RF /",
            "ChMoD 777 file",
            "SUDO rm file",
            "DD if=/dev/zero",
        ];

        for cmd in dangerous_commands {
            assert!(check_command_security(cmd).is_dangerous);
        }
    }

    #[test]
    fn test_content_security_analysis() {
        let content = "Run ![ls -la] and ![rm -rf /] and ![echo hello]";
        let results = analyze_content_security(content);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].severity, SecuritySeverity::Critical);
    }

    #[test]
    fn test_edge_cases() {
        // Empty command
        assert!(!check_command_security("").is_dangerous);

        // Command with similar but not matching pattern
        assert!(!check_command_security("rm_file").is_dangerous);

        // Command with extra spaces
        assert!(check_command_security("rm    -rf    /").is_dangerous);
    }

    #[test]
    fn test_false_positives() {
        // These should not be flagged as dangerous
        let safe_commands = [
            "rm my_file.txt",
            "chmod 755 script.sh",
            "echo 'rm -rf /'",
            "grep 'format' README.md",
            "list_chmod_options",
        ];

        for cmd in safe_commands {
            assert!(
                !check_command_security(cmd).is_dangerous,
                "Command '{}' should not be dangerous",
                cmd
            );
        }
    }

    #[test]
    fn test_false_negatives() {
        // These should be flagged as dangerous
        let dangerous_commands = [
            "rm -rf /etc",
            "chmod 777 important.txt",
            "sudo rm /boot/config",
            "dd if=/dev/sda of=/dev/null",
        ];

        for cmd in dangerous_commands {
            assert!(
                check_command_security(cmd).is_dangerous,
                "Command '{}' should be dangerous",
                cmd
            );
        }
    }
}
