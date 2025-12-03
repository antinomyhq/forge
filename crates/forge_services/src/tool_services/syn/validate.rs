use std::path::Path;

use thiserror::Error;
use tree_sitter::{Language, LanguageError, Parser};
use tree_sitter_md::LANGUAGE as MD_LANGUAGE;
use tree_sitter_sequel::LANGUAGE as SQL_LANGUAGE;
use tree_sitter_toml_ng::LANGUAGE as TOML_LANGUAGE;

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
        "Syntax validation failed for {file_path} ({extension}): The file was written successfully but contains syntax errors. Suggestion: Review and fix the syntax issues, or retry with properly escaped characters if HTML encoding was used."
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
/// * C# (.cs)
/// * C (.c, .h)
/// * PHP (.php)
/// * Swift (.swift)
/// * Kotlin (.kt, .kts)
/// * Dart (.dart)
/// * YAML (.yml, .yaml)
/// * TOML (.toml)
/// * Bash (.sh, .bash)
/// * HTML (.html, .htm)
/// * JSON (.json)
/// * SQL (.sql)
/// * Ruby (.rb)
/// * Markdown (.md, .markdown)
/// * PowerShell (.ps1, .psm1)
/// * C++ (.cpp, .cc, .cxx, .c++)
/// * CSS (.css)
/// * Go (.go)
/// * Java (.java)
/// * Scala (.scala)
pub fn extension(ext: &str) -> Option<Language> {
    match ext.to_lowercase().as_str() {
        // Existing languages
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

        // Phase 1 - New languages
        "cs" | "csx" => Some(tree_sitter_c_sharp::LANGUAGE.into()),
        "c" | "h" => Some(tree_sitter_c::LANGUAGE.into()),
        "php" => Some(tree_sitter_php::LANGUAGE_PHP.into()), // Fixed: Use LANGUAGE_PHP constant
        "swift" => Some(tree_sitter_swift::LANGUAGE.into()),
        "kt" | "kts" => Some(tree_sitter_kotlin_ng::LANGUAGE.into()),
        "dart" => Some(tree_sitter_dart::language()), // Correct: Uses language() function
        "yml" | "yaml" => Some(tree_sitter_yaml::LANGUAGE.into()),
        "toml" => Some(TOML_LANGUAGE.into()), // Fixed: Use tree-sitter-toml-ng
        // tree-sitter
        "sh" | "bash" | "zsh" | "fish" => Some(tree_sitter_bash::LANGUAGE.into()),
        "html" | "htm" | "xhtml" => Some(tree_sitter_html::LANGUAGE.into()),
        "json" => Some(tree_sitter_json::LANGUAGE.into()),
        "sql" => Some(SQL_LANGUAGE.into()),
        "md" | "markdown" => Some(MD_LANGUAGE.into()), /* Fixed: Use tree-sitter-md */
        // with LANGUAGE constant
        "ps1" | "psm1" | "psd1" => Some(tree_sitter_powershell::LANGUAGE.into()),

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

    #[tokio::test]
    async fn test_rust_valid() {
        let content = forge_test_kit::fixture!("/src/tool_services/syn/lang/rust/valid.rs").await;

        let path = PathBuf::from("test.rs");
        assert!(validate(&path, &content).is_none());
    }

    #[tokio::test]
    async fn test_rust_invalid() {
        let content = forge_test_kit::fixture!("/src/tool_services/syn/lang/rust/invalid.rs").await;

        let path = PathBuf::from("test.rs");
        let result = validate(&path, &content);
        assert!(matches!(result, Some(Error::Parse { .. })));
    }

    #[tokio::test]
    async fn test_javascript_valid() {
        let content =
            forge_test_kit::fixture!("/src/tool_services/syn/lang/javascript/valid.js").await;

        let path = PathBuf::from("test.js");
        assert!(validate(&path, &content).is_none());
    }

    #[tokio::test]
    async fn test_javascript_invalid() {
        let content =
            forge_test_kit::fixture!("/src/tool_services/syn/lang/javascript/invalid.js").await;

        let path = PathBuf::from("test.js");
        let result = validate(&path, &content);
        assert!(matches!(result, Some(Error::Parse { .. })));
    }

    #[tokio::test]
    async fn test_python_valid() {
        let content = forge_test_kit::fixture!("/src/tool_services/syn/lang/python/valid.py").await;

        let path = PathBuf::from("test.py");
        assert!(validate(&path, &content).is_none());
    }

    #[tokio::test]
    async fn test_python_invalid() {
        let content =
            forge_test_kit::fixture!("/src/tool_services/syn/lang/python/invalid.py").await;

        let path = PathBuf::from("test.py");
        let result = validate(&path, &content);
        assert!(matches!(result, Some(Error::Parse { .. })));
    }

    #[tokio::test]
    async fn test_c_sharp_valid() {
        let content =
            forge_test_kit::fixture!("/src/tool_services/syn/lang/c_sharp/valid.cs").await;

        let path = PathBuf::from("test.cs");
        assert!(validate(&path, &content).is_none());
    }

    #[tokio::test]
    async fn test_c_sharp_invalid() {
        let content =
            forge_test_kit::fixture!("/src/tool_services/syn/lang/c_sharp/invalid.cs").await;

        let path = PathBuf::from("test.cs");
        let result = validate(&path, &content);
        assert!(matches!(result, Some(Error::Parse { .. })));
    }

    #[tokio::test]
    async fn test_c_valid() {
        let content = forge_test_kit::fixture!("/src/tool_services/syn/lang/c/valid.c").await;

        let path = PathBuf::from("test.c");
        assert!(validate(&path, &content).is_none());
    }

    #[tokio::test]
    async fn test_c_invalid() {
        let content = forge_test_kit::fixture!("/src/tool_services/syn/lang/c/invalid.c").await;

        let path = PathBuf::from("test.c");
        let result = validate(&path, &content);
        assert!(matches!(result, Some(Error::Parse { .. })));
    }

    #[tokio::test]
    async fn test_swift_valid() {
        let content =
            forge_test_kit::fixture!("/src/tool_services/syn/lang/swift/valid.swift").await;

        let path = PathBuf::from("test.swift");
        assert!(validate(&path, &content).is_none());
    }

    #[tokio::test]
    async fn test_swift_invalid() {
        let content =
            forge_test_kit::fixture!("/src/tool_services/syn/lang/swift/invalid.swift").await;

        let path = PathBuf::from("test.swift");
        let result = validate(&path, &content);
        assert!(matches!(result, Some(Error::Parse { .. })));
    }

    #[tokio::test]
    async fn test_kotlin_valid() {
        let content = forge_test_kit::fixture!("/src/tool_services/syn/lang/kotlin/valid.kt").await;

        let path = PathBuf::from("test.kt");
        assert!(validate(&path, &content).is_none());
    }

    #[tokio::test]
    async fn test_kotlin_invalid() {
        let content =
            forge_test_kit::fixture!("/src/tool_services/syn/lang/kotlin/invalid.kt").await;

        let path = PathBuf::from("test.kt");
        let result = validate(&path, &content);
        assert!(matches!(result, Some(Error::Parse { .. })));
    }

    #[tokio::test]
    async fn test_yaml_valid() {
        let content = forge_test_kit::fixture!("/src/tool_services/syn/lang/yaml/valid.yaml").await;

        let path = PathBuf::from("test.yaml");
        assert!(validate(&path, &content).is_none());
    }

    #[tokio::test]
    async fn test_yaml_invalid() {
        let content =
            forge_test_kit::fixture!("/src/tool_services/syn/lang/yaml/invalid.yaml").await;

        let path = PathBuf::from("test.yaml");
        let result = validate(&path, &content);
        assert!(matches!(result, Some(Error::Parse { .. })));
    }

    #[tokio::test]
    async fn test_bash_valid() {
        let content = forge_test_kit::fixture!("/src/tool_services/syn/lang/bash/valid.sh").await;

        let path = PathBuf::from("test.sh");
        assert!(validate(&path, &content).is_none());
    }

    #[tokio::test]
    async fn test_bash_invalid() {
        let content = forge_test_kit::fixture!("/src/tool_services/syn/lang/bash/invalid.sh").await;

        let path = PathBuf::from("test.sh");
        let result = validate(&path, &content);
        assert!(matches!(result, Some(Error::Parse { .. })));
    }

    #[tokio::test]
    async fn test_html_valid() {
        let content = forge_test_kit::fixture!("/src/tool_services/syn/lang/html/valid.html").await;

        let path = PathBuf::from("test.html");
        assert!(validate(&path, &content).is_none());
    }

    #[tokio::test]
    async fn test_html_invalid() {
        let content =
            forge_test_kit::fixture!("/src/tool_services/syn/lang/html/invalid.html").await;

        let path = PathBuf::from("test.html");
        let result = validate(&path, &content);
        assert!(matches!(result, Some(Error::Parse { .. })));
    }

    #[tokio::test]
    async fn test_json_valid() {
        let content = forge_test_kit::fixture!("/src/tool_services/syn/lang/json/valid.json").await;

        let path = PathBuf::from("test.json");
        assert!(validate(&path, &content).is_none());
    }

    #[tokio::test]
    async fn test_json_invalid() {
        let content =
            forge_test_kit::fixture!("/src/tool_services/syn/lang/json/invalid.json").await;

        let path = PathBuf::from("test.json");
        let result = validate(&path, &content);
        assert!(matches!(result, Some(Error::Parse { .. })));
    }

    #[tokio::test]
    async fn test_powershell_valid() {
        let content =
            forge_test_kit::fixture!("/src/tool_services/syn/lang/powershell/valid.ps1").await;

        let path = PathBuf::from("test.ps1");
        assert!(validate(&path, &content).is_none());
    }

    #[tokio::test]
    async fn test_powershell_invalid() {
        let content =
            forge_test_kit::fixture!("/src/tool_services/syn/lang/powershell/invalid.ps1").await;

        let path = PathBuf::from("test.ps1");
        let result = validate(&path, &content);
        assert!(matches!(result, Some(Error::Parse { .. })));
    }

    // Test multiple file extensions
    #[tokio::test]
    async fn test_kotlin_kts_extension() {
        let content = forge_test_kit::fixture!("/src/tool_services/syn/lang/kotlin/valid.kt").await;

        let path = PathBuf::from("test.kts");
        assert!(validate(&path, &content).is_none());
    }

    #[tokio::test]
    async fn test_c_sharp_csx_extension() {
        let content =
            forge_test_kit::fixture!("/src/tool_services/syn/lang/c_sharp/valid.cs").await;

        let path = PathBuf::from("test.csx");
        assert!(validate(&path, &content).is_none());
    }

    #[tokio::test]
    async fn test_powershell_psm1_extension() {
        let content =
            forge_test_kit::fixture!("/src/tool_services/syn/lang/powershell/valid.ps1").await;

        let path = PathBuf::from("test.psm1");
        assert!(validate(&path, &content).is_none());
    }

    #[tokio::test]
    async fn test_yaml_yml_extension() {
        let content = forge_test_kit::fixture!("/src/tool_services/syn/lang/yaml/valid.yaml").await;

        let path = PathBuf::from("test.yml");
        assert!(validate(&path, &content).is_none());
    }

    #[tokio::test]
    async fn test_html_htm_extension() {
        let content = forge_test_kit::fixture!("/src/tool_services/syn/lang/html/valid.html").await;

        let path = PathBuf::from("test.htm");
        assert!(validate(&path, &content).is_none());
    }

    #[tokio::test]
    async fn test_bash_bash_extension() {
        let content = forge_test_kit::fixture!("/src/tool_services/syn/lang/bash/valid.sh").await;

        let path = PathBuf::from("test.bash");
        assert!(validate(&path, &content).is_none());
    }

    #[tokio::test]
    async fn test_case_insensitive_extensions() {
        let content =
            forge_test_kit::fixture!("/src/tool_services/syn/lang/c_sharp/valid.cs").await;

        let path = PathBuf::from("test.CS");
        assert!(validate(&path, &content).is_none());
    }

    #[test]
    fn test_extension_mapping() {
        // Test existing languages still work
        assert!(extension("rs").is_some());
        assert!(extension("py").is_some());
        assert!(extension("js").is_some());
        assert!(extension("cpp").is_some());

        // Test new languages that work
        assert!(extension("cs").is_some());
        assert!(extension("c").is_some());
        assert!(extension("swift").is_some());
        assert!(extension("kt").is_some());
        assert!(extension("yaml").is_some());
        assert!(extension("sh").is_some());
        assert!(extension("html").is_some());
        assert!(extension("json").is_some());
        assert!(extension("ps1").is_some());

        // Test multiple extensions
        assert!(extension("csx").is_some());
        assert!(extension("kts").is_some());
        assert!(extension("psm1").is_some());
        assert!(extension("bash").is_some());
        assert!(extension("zsh").is_some());
        assert!(extension("htm").is_some());
        assert!(extension("xhtml").is_some());
        assert!(extension("yml").is_some());

        // Test case insensitive
        assert!(extension("RS").is_some());
        assert!(extension("PY").is_some());
        assert!(extension("CS").is_some());

        // Test unsupported extensions
        assert!(extension("unknown").is_none());
        assert!(extension("txt").is_none());

        // Test languages with API issues (all now resolved!)
        // TOML now works with tree-sitter-toml-ng!
        assert!(extension("toml").is_some());
        assert!(extension("md").is_some()); // Fixed: Now works with tree-sitter-md

        // SQL now works with tree-sitter-sequel
        assert!(extension("sql").is_some());
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
            "Syntax validation failed for test.rs (rs): The file was written successfully but contains syntax errors. Suggestion: Review and fix the syntax issues, or retry with properly escaped characters if HTML encoding was used."
        );
    }
}

#[tokio::test]
async fn test_powershell_invalid() {
    let source =
        forge_test_kit::fixture!("/src/tool_services/syn/lang/powershell/invalid.ps1").await;
    let result = validate("test.ps1", &source);
    assert!(result.is_some());
}

#[tokio::test]
async fn test_php_valid() {
    let source = forge_test_kit::fixture!("/src/tool_services/syn/lang/php/valid.php").await;
    let result = validate("test.php", &source);
    assert!(result.is_none());
}

#[tokio::test]
async fn test_php_invalid() {
    let source = forge_test_kit::fixture!("/src/tool_services/syn/lang/php/invalid.php").await;
    let result = validate("test.php", &source);
    assert!(result.is_some());
}

#[tokio::test]
async fn test_dart_valid() {
    let source = forge_test_kit::fixture!("/src/tool_services/syn/lang/dart/valid.dart").await;
    let result = validate("test.dart", &source);
    assert!(result.is_none());
}

#[tokio::test]
async fn test_dart_invalid() {
    let source = forge_test_kit::fixture!("/src/tool_services/syn/lang/dart/invalid.dart").await;
    let result = validate("test.dart", &source);
    assert!(result.is_some());
}

#[tokio::test]
async fn test_toml_valid() {
    let source = forge_test_kit::fixture!("/src/tool_services/syn/lang/toml/valid.toml").await;
    let result = validate("test.toml", &source);
    assert!(result.is_none());
}

#[tokio::test]
async fn test_toml_invalid() {
    let source = forge_test_kit::fixture!("/src/tool_services/syn/lang/toml/invalid.toml").await;
    let result = validate("test.toml", &source);
    assert!(result.is_some());
}

#[tokio::test]
async fn test_sql_valid() {
    let source = forge_test_kit::fixture!("/src/tool_services/syn/lang/sql/valid.sql").await;
    let result = validate("test.sql", &source);
    assert!(result.is_none());
}

#[tokio::test]
async fn test_sql_invalid() {
    let source = forge_test_kit::fixture!("/src/tool_services/syn/lang/sql/invalid.sql").await;
    let result = validate("test.sql", &source);
    assert!(result.is_some());
}

#[tokio::test]
async fn test_ruby_valid() {
    let source = forge_test_kit::fixture!("/src/tool_services/syn/lang/ruby/valid.rb").await;
    let result = validate("test.rb", &source);
    assert!(result.is_none());
}

#[tokio::test]
async fn test_ruby_invalid() {
    let source = forge_test_kit::fixture!("/src/tool_services/syn/lang/ruby/invalid.rb").await;
    let result = validate("test.rb", &source);
    assert!(result.is_some());
}

#[tokio::test]
async fn test_markdown_valid() {
    let source = forge_test_kit::fixture!("/src/tool_services/syn/lang/markdown/valid.md").await;
    let result = validate("test.md", &source);
    assert!(result.is_none());
}

#[tokio::test]
async fn test_markdown_invalid() {
    let source = forge_test_kit::fixture!("/src/tool_services/syn/lang/markdown/invalid.md").await;
    let result = validate("test.md", &source);
    assert!(result.is_some());
}
