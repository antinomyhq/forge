//! Centralized display constants for consistent UI messaging
//!
//! This module provides a single source of truth for all display values used
//! across the application. When adding new constants, follow these conventions:
//!
//! - **Placeholders** (missing/unknown data): Use angle brackets `<value>`
//! - **Special markers**: Use square brackets `[value]`
//! - **Status values**: Use lowercase for user-facing strings
//! - **Type discriminators**: Use enums instead of string constants
//!
//! # Examples
//!
//! ```rust,ignore
//! use crate::display_constants::{placeholders, status};
//!
//! // For missing data
//! let value = some_optional.unwrap_or(placeholders::UNKNOWN);
//!
//! // For status indicators
//! info.add_key_value("status", status::AVAILABLE);
//! ```

use std::fmt;

/// Placeholder values for missing, unknown, or unavailable data.
///
/// These constants use angle brackets to indicate placeholder values
/// (e.g., `<unknown>`, `<not set>`).
pub mod placeholders {

    /// Indicates an empty value (distinct from null/unset)
    pub const EMPTY: &str = "[empty]";
}

/// Status indicator values.
///
/// Use lowercase for user-facing status strings to maintain consistency.
pub mod status {
    /// Indicates a resource is available/configured
    pub const AVAILABLE: &str = "[available]";

    /// Indicates a resource is disabled
    pub const DISABLED: &str = "[disabled]";
}

/// Table column headers for porcelain (machine-readable) output.
///
/// These headers use the `$` prefix to distinguish them as metadata columns.
pub mod headers {
    /// Default ID column header
    pub const ID: &str = "$ID";

    /// Field name column header
    pub const FIELD: &str = "$FIELD";

    /// Field value column header
    pub const VALUE: &str = "$VALUE";
}

/// Special markers for specific contexts.
///
/// These use square brackets to indicate special/synthetic values.
pub mod markers {
    /// Indicates a built-in (non-user-defined) component
    pub const BUILT_IN: &str = "[built-in]";
}

/// Type discriminator for commands, agents, and custom entries.
///
/// Use this enum instead of string constants for type-safe discrimination
/// between command types in listings and displays.
///
/// # Examples
///
/// ```rust,ignore
/// let cmd_type = CommandType::Agent;
/// info.add_key_value("type", cmd_type.as_str());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandType {
    /// Built-in command
    Command,
    /// Agent (AI assistant with specific role)
    Agent,
    /// User-defined custom command
    Custom,
}

impl CommandType {
    /// Returns the string representation of the command type.
    ///
    /// This is the canonical way to convert a CommandType to a string
    /// for display purposes.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Agent => "agent",
            Self::Custom => "custom",
        }
    }
}

impl fmt::Display for CommandType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_type_display() {
        assert_eq!(CommandType::Command.to_string(), "command");
        assert_eq!(CommandType::Agent.to_string(), "agent");
        assert_eq!(CommandType::Custom.to_string(), "custom");
    }

    #[test]
    fn test_command_type_as_str() {
        assert_eq!(CommandType::Command.as_str(), "command");
        assert_eq!(CommandType::Agent.as_str(), "agent");
        assert_eq!(CommandType::Custom.as_str(), "custom");
    }

    #[test]
    fn test_placeholders_have_angle_brackets() {
        assert!(placeholders::EMPTY.starts_with('<'));
        assert!(placeholders::EMPTY.starts_with('<'));
        assert!(placeholders::EMPTY.starts_with('<'));
    }

    #[test]
    fn test_markers_have_square_brackets() {
        assert!(markers::BUILT_IN.starts_with('['));
    }
}
