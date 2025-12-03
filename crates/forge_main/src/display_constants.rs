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
//! // For special values
//! let empty = placeholders::EMPTY;  // "[empty]"
//!
//! // For status indicators
//! info.add_key_value("status", status::ENABLED);  // "[enabled]"
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
    /// Indicates a resource is enabled/configured
    pub const ENABLED: &str = "[enabled]";

    /// Indicates a resource is disabled
    pub const DISABLED: &str = "[disabled]";

    /// Indicates a feature is supported
    pub const SUPPORTED: &str = "[supported]";

    /// Indicates a feature is not supported
    pub const UNSUPPORTED: &str = "[unsupported]";
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

    /// Indicates a resource is unavailable (e.g., provider not configured)
    pub const UNAVAILABLE: &str = "[unavailable]";

    /// Indicates the active/current selection
    pub const ACTIVE: &str = "[ACTIVE]";
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
    fn test_placeholders_use_square_brackets() {
        // EMPTY uses square brackets like other special markers
        assert!(placeholders::EMPTY.starts_with('['));
        assert!(placeholders::EMPTY.ends_with(']'));
    }

    #[test]
    fn test_markers_have_square_brackets() {
        assert!(markers::BUILT_IN.starts_with('['));
        assert!(markers::BUILT_IN.ends_with(']'));
        assert!(markers::UNAVAILABLE.starts_with('['));
        assert!(markers::UNAVAILABLE.ends_with(']'));
        assert!(markers::ACTIVE.starts_with('['));
        assert!(markers::ACTIVE.ends_with(']'));
    }

    #[test]
    fn test_status_values_use_square_brackets() {
        // Status values use square brackets to distinguish them from raw strings
        assert!(status::ENABLED.starts_with('['));
        assert!(status::ENABLED.ends_with(']'));
        assert!(status::DISABLED.starts_with('['));
        assert!(status::DISABLED.ends_with(']'));
        assert!(status::SUPPORTED.starts_with('['));
        assert!(status::SUPPORTED.ends_with(']'));
        assert!(status::UNSUPPORTED.starts_with('['));
        assert!(status::UNSUPPORTED.ends_with(']'));
    }
}
