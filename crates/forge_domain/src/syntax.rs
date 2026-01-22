use serde::{Deserialize, Serialize};

/// Information about a supported programming language syntax
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxInfo {
    /// Language name (e.g., "Rust", "Python", "TypeScript")
    pub language: String,
    /// File extensions for this language (e.g., [".rs"], [".py"], [".ts",
    /// ".tsx"])
    pub extensions: Vec<String>,
}

impl SyntaxInfo {
    /// Create a new syntax info entry
    pub fn new(language: String, extensions: Vec<String>) -> Self {
        Self { language, extensions }
    }
}
