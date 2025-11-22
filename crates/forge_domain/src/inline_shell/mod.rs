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
    /// Critical risk - can completely compromise system
    Critical,
}

/// Security detection result for a command
#[derive(Debug, Clone)]
pub struct SecurityCheckResult {
    /// Whether command is considered dangerous
    pub is_dangerous: bool,
    /// Security severity level
    pub severity: SecuritySeverity,
    /// Reason for security classification
    pub reason: String,
    /// Matched pattern that triggered security check
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

/// Gets compiled security patterns
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
                regex: Regex::new(r"(?i)^\s*dd\s+if=/dev/").unwrap(),
                severity: SecuritySeverity::Medium,
                reason: "Can overwrite disk devices",
            },
            SecurityPattern {
                regex: Regex::new(r"(?i)^\s*mkfs").unwrap(),
                severity: SecuritySeverity::Medium,
                reason: "Can create filesystems",
            },
            SecurityPattern {
                regex: Regex::new(r"(?i)^\s*format").unwrap(),
                severity: SecuritySeverity::Medium,
                reason: "Can format disks",
            },
            // Low severity patterns
            SecurityPattern {
                regex: Regex::new(r"(?i)^\s*reboot").unwrap(),
                severity: SecuritySeverity::Low,
                reason: "Can restart system",
            },
            SecurityPattern {
                regex: Regex::new(r"(?i)^\s*shutdown").unwrap(),
                severity: SecuritySeverity::Low,
                reason: "Can shutdown system",
            },
            SecurityPattern {
                regex: Regex::new(r"(?i)^\s*halt").unwrap(),
                severity: SecuritySeverity::Low,
                reason: "Can halt system",
            },
            SecurityPattern {
                regex: Regex::new(r"(?i)^\s*poweroff").unwrap(),
                severity: SecuritySeverity::Low,
                reason: "Can power off system",
            },
        ]
    })
}

/// Checks if a command is dangerous based on security patterns
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
        matched_pattern: "".to_string(),
    }
}

/// Analyzes content for dangerous inline shell commands
pub fn analyze_content_security(content: &str) -> Vec<SecurityCheckResult> {
    use crate::inline_shell::parser::parse_inline_commands;
    
    let parsed_content = match parse_inline_commands(content) {
        Ok(parsed) => parsed,
        Err(_) => return Vec::new(),
    };

    parsed_content
        .commands_found
        .iter()
        .map(|cmd| check_command_security(&cmd.command))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_command_safe_command() {
        let result = check_command_security("echo hello");
        assert!(!result.is_dangerous);
        assert_eq!(result.severity, SecuritySeverity::Low);
    }

    #[test]
    fn test_check_command_dangerous_critical() {
        let result = check_command_security("rm -rf /");
        assert!(result.is_dangerous);
        assert_eq!(result.severity, SecuritySeverity::Critical);
        assert_eq!(result.reason, "Can delete entire filesystem");
    }

    #[test]
    fn test_check_command_dangerous_high() {
        let result = check_command_security("chmod 777 file.txt");
        assert!(result.is_dangerous);
        assert_eq!(result.severity, SecuritySeverity::High);
    }

    #[test]
    fn test_analyze_content_security() {
        let content = "Safe text ![echo hello] dangerous ![rm -rf /] end";
        let results = analyze_content_security(content);
        assert_eq!(results.len(), 2);
        assert!(!results[0].is_dangerous); // echo hello
        assert!(results[1].is_dangerous);  // rm -rf /
    }
}