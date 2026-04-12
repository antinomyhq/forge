/// Represents the terminal context captured by the shell plugin.
///
/// Contains the serialized string of recent shell commands, exit codes, and
/// terminal output that the zsh plugin exports via the `FORGE_TERM_CONTEXT`
/// environment variable before invoking forge.
#[derive(Debug, Clone, PartialEq, Eq)]
// FIXME: Add fields to extract data for each env variable instead of a concatenated text
pub struct TerminalContext(String);

impl TerminalContext {
    /// Creates a new `TerminalContext` from a raw string.
    pub fn new(content: impl Into<String>) -> Self {
        Self(content.into())
    }

    /// Returns the raw context string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for TerminalContext {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for TerminalContext {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}
