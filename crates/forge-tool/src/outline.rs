use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use schemars::JsonSchema;
use serde::Deserialize;
use tokio::fs;
use tree_sitter::{Language, Parser, Query, QueryCursor};
use walkdir::WalkDir;

use crate::{Description, ToolTrait};

const JAVASCRIPT: &str = include_str!("queries/javascript.rkt");
const PYTHON: &str = include_str!("queries/python.rkt");
const RUST: &str = include_str!("queries/rust.rkt");

fn load_language_parser(language_name: &str) -> Result<Language, String> {
    match language_name {
        "rust" => Ok(tree_sitter_rust::language()),
        "javascript" => Ok(tree_sitter_javascript::language()),
        "python" => Ok(tree_sitter_python::language()),
        x => Err(format!("Unsupported language: {}", x)),
    }
}

fn load_queries() -> BTreeMap<&'static str, &'static str> {
    let mut queries = BTreeMap::new();
    queries.insert("rust", RUST);
    queries.insert("javascript", JAVASCRIPT);
    queries.insert("python", PYTHON);
    queries
}

fn parse_file(_file: &Path, content: &str, parser: &mut Parser, query: &Query) -> Option<String> {
    let tree = parser.parse(content, None)?;
    let mut cursor = QueryCursor::new();
    let mut formatted_output = String::new();
    let mut last_line = -1;
    let mut seen_lines = BTreeSet::new();

    let mut captures: Vec<_> = cursor
        .matches(query, tree.root_node(), content.as_bytes())
        .flat_map(|m| m.captures)
        .filter_map(|capture| {
            let node = capture.node;
            let start_line = node.start_position().row;
            // Get the full text of the node instead of just the first line
            let text = node.utf8_text(content.as_bytes()).ok()?;
            // Get the first line of the definition which contains the signature
            let first_line = text.lines().next()?.trim().to_string();
            Some((start_line, first_line))
        })
        .collect();

    captures.sort_by_key(|&(row, _)| row);

    for (start_line, text) in captures {
        if !seen_lines.insert(start_line) {
            continue;
        }

        if last_line != -1 && start_line as i32 > last_line + 1 {
            formatted_output.push_str("|----\n");
        }

        formatted_output.push_str(&format!("│{}\n", text.trim()));
        last_line = start_line as i32;
    }

    if formatted_output.is_empty() {
        None
    } else {
        Some(formatted_output)
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct OutlineInput {
    pub path: String,
}

pub(crate) struct Outline;

impl Description for Outline {
    fn description() -> &'static str {
        "List definition names (classes, functions, methods, etc.) used in source code files. \
        Provides insights into codebase structure and important constructs. Supports multiple \
        programming languages including Rust, JavaScript, and Python. Returns a formatted \
        string showing file names and their definitions."
    }
}

#[async_trait::async_trait]
impl ToolTrait for Outline {
    type Input = OutlineInput;
    type Output = String;

    async fn call(&self, input: Self::Input) -> Result<Self::Output, String> {
        let extensions_to_languages =
            BTreeMap::from([("rs", "rust"), ("js", "javascript"), ("py", "python")]);

        let queries = load_queries();
        let mut parsers: BTreeMap<&str, (Parser, Query)> = BTreeMap::new();
        let mut result = String::new();

        let entries = WalkDir::new(&input.path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_type().is_file()
                    && e.path()
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|ext| {
                            extensions_to_languages.contains_key(ext.to_lowercase().as_str())
                        })
                        .unwrap_or(false)
            })
            .collect::<Vec<_>>();

        for entry in entries {
            let path = entry.path().to_path_buf();
            if let Ok(content) = fs::read_to_string(&path).await {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if let Some(&lang_name) =
                        extensions_to_languages.get(ext.to_lowercase().as_str())
                    {
                        if !parsers.contains_key(lang_name) {
                            let language = load_language_parser(lang_name)?;
                            let mut parser = Parser::new();
                            parser.set_language(language).map_err(|e| e.to_string())?;
                            let query = Query::new(language, queries[lang_name])
                                .map_err(|e| e.to_string())?;
                            parsers.insert(lang_name, (parser, query));
                        }

                        if let Some((parser, query)) = parsers.get_mut(lang_name) {
                            if let Some(file_output) = parse_file(&path, &content, parser, query) {
                                if !result.is_empty() {
                                    result.push_str("|----\n");
                                }
                                result.push_str(&format!(
                                    "{}\n",
                                    path.file_name().unwrap().to_string_lossy()
                                ));
                                result.push_str(&file_output);
                            }
                        }
                    }
                }
            }
        }

        if result.is_empty() {
            Ok("No source code definitions found.".into())
        } else {
            Ok(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use tempfile::TempDir;
    use tokio::fs;

    use super::*;

    #[tokio::test]
    async fn test_outline_rust() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
struct User {
    name: String,
    age: u32,
}

fn calculate_age(birth_year: u32) -> u32 {
    2024 - birth_year
}

impl User {
    fn new(name: String, age: u32) -> Self {
        User { name, age }
    }
}
"#;
        let file_path = temp_dir.path().join("test.rs");
        fs::write(&file_path, content).await.unwrap();

        let outline = Outline;
        let result = outline
            .call(OutlineInput { path: temp_dir.path().to_string_lossy().to_string() })
            .await
            .unwrap();

        assert_snapshot!("outline_rust", result);
    }

    #[tokio::test]
    async fn test_outline_javascript() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
function calculateTotal(items) {
    return items.reduce((sum, item) => sum + item.price, 0);
}

class ShoppingCart {
    constructor() {
        this.items = [];
    }

    addItem(item) {
        this.items.push(item);
    }
}
"#;
        let file_path = temp_dir.path().join("test.js");
        fs::write(&file_path, content).await.unwrap();

        let outline = Outline;
        let result = outline
            .call(OutlineInput { path: temp_dir.path().to_string_lossy().to_string() })
            .await
            .unwrap();

        assert_snapshot!("outline_javascript", result);
    }

    #[tokio::test]
    async fn test_outline_python() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
def greet(name):
    return f"Hello, {name}!"

class Person:
    def __init__(self, name):
        self.name = name

    def say_hello(self):
        return greet(self.name)
"#;
        let file_path = temp_dir.path().join("test.py");
        fs::write(&file_path, content).await.unwrap();

        let outline = Outline;
        let result = outline
            .call(OutlineInput { path: temp_dir.path().to_string_lossy().to_string() })
            .await
            .unwrap();

        assert_snapshot!("outline_python", result);
    }

    #[tokio::test]
    async fn test_outline_multiple_files() {
        let temp_dir = TempDir::new().unwrap();

        // Rust file
        fs::write(
            temp_dir.path().join("main.rs"),
            "fn main() { println!(\"Hello\"); }",
        )
        .await
        .unwrap();

        // JavaScript file
        fs::write(
            temp_dir.path().join("script.js"),
            "function init() { console.log('Ready'); }",
        )
        .await
        .unwrap();

        // Python file
        fs::write(
            temp_dir.path().join("app.py"),
            "def start(): print('Starting')",
        )
        .await
        .unwrap();

        let outline = Outline;
        let result = outline
            .call(OutlineInput { path: temp_dir.path().to_string_lossy().to_string() })
            .await
            .unwrap();

        assert_snapshot!("outline_multiple_files", result);
    }

    #[tokio::test]
    async fn test_outline_empty_directory() {
        let temp_dir = TempDir::new().unwrap();

        let outline = Outline;
        let result = outline
            .call(OutlineInput { path: temp_dir.path().to_string_lossy().to_string() })
            .await
            .unwrap();

        assert_snapshot!("outline_empty_directory", result);
    }

    #[tokio::test]
    async fn test_outline_unsupported_files() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("data.txt"), "Some text")
            .await
            .unwrap();

        let outline = Outline;
        let result = outline
            .call(OutlineInput { path: temp_dir.path().to_string_lossy().to_string() })
            .await
            .unwrap();

        assert_snapshot!("outline_unsupported_files", result);
    }
}
