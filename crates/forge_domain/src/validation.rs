/// Represents a single syntax error in a file
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    /// Line number where the error occurred (1-based)
    pub line: u32,
    /// Column number where the error occurred (1-based)
    pub column: u32,
    /// Error message describing the syntax issue
    pub message: String,
}
