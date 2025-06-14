use std::path::Path;

use thiserror::Error;
use tree_sitter::{Language, LanguageError, Parser};

/// Represents possible errors that can occur during syntax validation
#[derive(Debug, Error, PartialEq)]
pub enum Error {
    /// The file has no extension
    #[error("File has no extension")]
    Extension,
    /// Failed to initialize the parser with the specified language
    #[error("Parser initialization error: {0}")]
    Language(#[from] LanguageError),
    /// Failed to parse the content
    #[error(
        "Syntax error found in file with extension {extension}. Hint: Please retry in raw mode without HTML-encoding angle brackets."
    )]
    Parse {
        file_path: String,
        extension: String,
    },
}

/// Maps file extensions to their corresponding Tree-sitter language parsers.
///
/// This function takes a file extension as input and returns the appropriate
/// Tree-sitter language parser if supported.
///
/// # Arguments
/// * `ext` - The file extension to get a language parser for
///
/// # Returns
/// * `Some(Language)` - If the extension is supported
/// * `None` - If the extension is not supported
///
/// # Supported Languages
/// * Rust (.rs)
/// * JavaScript/TypeScript (.js, .jsx, .ts, .tsx)
/// * Python (.py)
pub fn extension(ext: &str) -> Option<Language> {
    match ext.to_lowercase().as_str() {
        "rs" => Some(tree_sitter_rust::LANGUAGE.into()),
        "py" => Some(tree_sitter_python::LANGUAGE.into()),
        "cpp" | "cc" | "cxx" | "c++" => Some(tree_sitter_cpp::LANGUAGE.into()),
        "css" => Some(tree_sitter_css::LANGUAGE.into()),
        "go" => Some(tree_sitter_go::LANGUAGE.into()),
        "java" => Some(tree_sitter_java::LANGUAGE.into()),
        "rb" => Some(tree_sitter_ruby::LANGUAGE.into()),
        "scala" => Some(tree_sitter_scala::LANGUAGE.into()),
        "ts" | "js" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "tsx" => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        _ => None,
    }
}

/// Validates source code content using Tree-sitter parsers.
///
/// This function attempts to parse the provided content using a Tree-sitter
/// parser appropriate for the file's extension. It checks for syntax errors in
/// the parsed abstract syntax tree.
///
/// # Arguments
/// * `path` - The path to the file being validated (used to determine language)
/// * `content` - The source code content to validate
///
/// # Returns
/// * `Ok(())` - If the content is valid for the given language
/// * `Err(String)` - If validation fails, contains error description
///
/// # Note
/// Files with unsupported extensions are considered valid and will return
/// Ok(()). Files with no extension will return an error.
pub fn validate(path: impl AsRef<Path>, content: &str) -> Option<Error> {
    let path = path.as_ref();

    // Get file extension
    let ext = match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => ext,
        None => return Some(Error::Extension),
    };

    // Get language for the extension
    // If we don't support the language, consider it valid
    let language = extension(ext)?;

    // Initialize parser
    let mut parser = Parser::new();
    if let Err(e) = parser.set_language(&language) {
        return Some(Error::Language(e));
    }

    // Try parsing the content
    let Some(tree) = parser.parse(content, None) else {
        return Some(Error::Parse {
            file_path: path.display().to_string(),
            extension: ext.to_string(),
        });
    };

    // Find syntax errors in the tree
    let root_node = tree.root_node();
    (root_node.has_error() || root_node.is_error()).then(|| Error::Parse {
        file_path: path.display().to_string(),
        extension: ext.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    // Include language samples
    const RUST_VALID: &str = include_str!("lang/rust/valid.rs");
    const RUST_INVALID: &str = include_str!("lang/rust/invalid.rs");
    const JAVASCRIPT_VALID: &str = include_str!("lang/javascript/valid.js");
    const JAVASCRIPT_INVALID: &str = include_str!("lang/javascript/invalid.js");
    const PYTHON_VALID: &str = include_str!("lang/python/valid.py");
    const PYTHON_INVALID: &str = include_str!("lang/python/invalid.py");

    #[test]
    fn test_rust_valid() {
        let path = PathBuf::from("test.rs");
        assert!(validate(&path, RUST_VALID).is_none());
    }

    #[test]
    fn test_rust_invalid() {
        let path = PathBuf::from("test.rs");
        let result = validate(&path, RUST_INVALID);
        assert!(matches!(result, Some(Error::Parse { .. })));
    }

    #[test]
    fn test_javascript_valid() {
        let path = PathBuf::from("test.js");
        assert!(validate(&path, JAVASCRIPT_VALID).is_none());
    }

    #[test]
    fn test_javascript_invalid() {
        let path = PathBuf::from("test.js");
        let result = validate(&path, JAVASCRIPT_INVALID);
        assert!(matches!(result, Some(Error::Parse { .. })));
    }

    #[test]
    fn test_python_valid() {
        let path = PathBuf::from("test.py");
        assert!(validate(&path, PYTHON_VALID).is_none());
    }

    #[test]
    fn test_python_invalid() {
        let path = PathBuf::from("test.py");
        let result = validate(&path, PYTHON_INVALID);
        assert!(matches!(result, Some(Error::Parse { .. })));
    }

    #[test]
    fn test_unsupported_extension() {
        let content = "Some random content";
        let path = PathBuf::from("test.txt");
        assert!(validate(&path, content).is_none());
    }

    #[test]
    fn test_no_extension() {
        let content = "Some random content";
        let path = PathBuf::from("test");
        let result = validate(&path, content);
        assert!(matches!(result, Some(Error::Extension)));
    }

    #[test]
    fn test_error_messages() {
        let path = PathBuf::from("test");
        let error = validate(&path, "").unwrap();
        assert_eq!(error.to_string(), "File has no extension");

        let path = PathBuf::from("test.rs");
        let error = validate(&path, "fn main() { let x = ").unwrap();
        assert_eq!(
            error.to_string(),
            "Syntax error found in file with extension rs. Hint: Please retry in raw mode without HTML-encoding angle brackets."
        );
    }
}
