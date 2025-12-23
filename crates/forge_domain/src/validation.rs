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

/// Warning information about validation failures
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationWarning {
    /// Path to the file that failed validation
    pub file_path: String,
    /// File extension (e.g., "rs", "py", "ts")
    pub extension: String,
    /// List of syntax errors found in the file
    pub errors: Vec<SyntaxError>,
}

impl ValidationWarning {
    /// Create a new validation warning
    ///
    /// # Arguments
    /// * `file_path` - Path to the file
    /// * `extension` - File extension
    /// * `errors` - List of syntax errors
    pub fn new(file_path: String, extension: String, errors: Vec<SyntaxError>) -> Self {
        Self { file_path, extension, errors }
    }

    /// Get the count of errors
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }
}

impl From<&ValidationWarning> for forge_template::Element {
    fn from(warning: &ValidationWarning) -> Self {
        use forge_template::Element;

        Element::new("warning")
            .append(Element::new("message").text("Syntax validation failed"))
            .append(
                Element::new("file")
                    .attr("path", &warning.file_path)
                    .attr("extension", &warning.extension),
            )
            .append(Element::new("details").text(format!(
                "The file was written successfully but contains {} syntax error(s)",
                warning.error_count()
            )))
            .append(warning.errors.iter().map(|error| {
                Element::new("error")
                    .attr("line", error.line.to_string())
                    .attr("column", error.column.to_string())
                    .cdata(&error.message)
            }))
            .append(Element::new("suggestion").text("Review and fix the syntax issues"))
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_warning() {
        let fixture = ValidationWarning::new(
            "/path/to/file.rs".to_string(),
            "rs".to_string(),
            vec![
                SyntaxError { line: 10, column: 5, message: "Missing semicolon".to_string() },
                SyntaxError { line: 15, column: 20, message: "Unexpected token".to_string() },
            ],
        );

        let actual = fixture.error_count();
        let expected = 2;

        assert_eq!(actual, expected);
        assert_eq!(fixture.file_path, "/path/to/file.rs");
        assert_eq!(fixture.extension, "rs");
    }
}
