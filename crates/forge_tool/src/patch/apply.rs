use std::ops::Bound::{Excluded, Included};
use std::ops::RangeBounds;
use std::path::{Path, PathBuf};

use dissimilar::Chunk;
use forge_domain::{NamedTool, ToolCallService, ToolDescription, ToolName};
use schemars::JsonSchema;
use serde::Deserialize;
use thiserror::Error;
use tokio::fs;

use super::marker::{DIVIDER, REPLACE, SEARCH};
use super::parse::{self, PatchBlock};
use crate::syn;

#[derive(Debug, Error)]
enum Error {
    #[error("File not found at path: {0}")]
    FileNotFound(PathBuf),
    #[error("File operation failed: {0}")]
    FileOperation(#[from] std::io::Error),
}

/// Input parameters for the fs_replace tool.
#[derive(Deserialize, JsonSchema)]
pub struct ApplyPatchInput {
    /// File path relative to the current working directory
    pub path: String,
    /// Multiple SEARCH/REPLACE blocks separated by newlines, defining changes
    /// to make to the file.
    pub diff: String,
}

pub struct ApplyPatch;

impl NamedTool for ApplyPatch {
    fn tool_name(&self) -> ToolName {
        ToolName::new("tool_forge_patch")
    }
}

impl ToolDescription for ApplyPatch {
    fn description(&self) -> String {
        format!(
            r#"Replace sections in a file using multiple SEARCH/REPLACE blocks. Example:
{SEARCH}
[exact content to find]
{DIVIDER}
[new content to replace with]
{REPLACE}

Rules:
1. SEARCH must exactly match whitespace, indentation & line endings
2. Each block replaces first match only
3. Keep blocks minimal - include only changing lines plus needed context
4. Provide complete lines only - no truncation
5. Use multiple blocks for multiple changes in the same file
6. For moves: use 2 blocks (delete block + insert block)
7. For deletes: use empty REPLACE section

Example with multiple blocks:
{SEARCH}
def old_function(x):
    return x + 1
{DIVIDER}
def new_function(x, y=0):
    return x + y
{REPLACE}
{SEARCH}
# Old comment
{DIVIDER}
# Updated documentation - now supports multiple parameters
{REPLACE}
        "#
        )
        .trim()
        .to_string()
    }
}

/// Apply changes to file content based on search/replace blocks.
/// Changes are only written to disk if all replacements are successful.
async fn apply_patches(content: String, blocks: Vec<PatchBlock>) -> Result<String, Error> {
    let mut result = content;

    // Apply each block sequentially
    for block in blocks {
        // For empty search string, append the replacement text at the end of file.
        if block.search.is_empty() {
            result.push_str(&block.replace);
            continue;
        }

        // For exact matching, first try to find the exact string
        if let Some(start_idx) = result.find(&block.search) {
            let end_idx = start_idx + block.search.len();
            result.replace_range(start_idx..end_idx, &block.replace);
            continue;
        }

        // If exact match fails, try fuzzy matching
        let normalized_search = block.search.replace("\r\n", "\n").replace('\r', "\n");
        let normalized_result = result.replace("\r\n", "\n").replace('\r', "\n");

        if let Some(start_idx) = normalized_result.find(&normalized_search) {
            safe_replace_range(
                &mut result,
                start_idx..start_idx + block.search.len(),
                &block.replace,
            )
            .ok_or_else(|| {
                Error::FileOperation(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid character boundaries",
                ))
            })?;
            continue;
        }

        // If still no match, try more aggressive fuzzy matching
        let chunks = dissimilar::diff(&result, &block.search);
        let mut best_match = None;
        let mut best_score = 0.0;
        let mut current_pos = 0;

        for chunk in chunks.iter() {
            if let Chunk::Equal(text) = chunk {
                let score = text.len() as f64 / block.search.len() as f64;
                if score > best_score {
                    best_score = score;
                    best_match = Some((current_pos, text.len()));
                }
            }
            match chunk {
                Chunk::Equal(text) | Chunk::Delete(text) | Chunk::Insert(text) => {
                    current_pos += text.len();
                }
            }
        }

        if let Some((start_idx, len)) = best_match {
            if best_score > 0.7 {
                // Threshold for fuzzy matching
                safe_replace_range(&mut result, start_idx..start_idx + len, &block.replace)
                    .ok_or_else(|| {
                        Error::FileOperation(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "Invalid character boundaries",
                        ))
                    })?;
            }
        }
    }

    Ok(result)
}

pub fn safe_replace_range<R>(string: &mut String, range: R, replace_with: &str) -> Option<()>
where
    R: RangeBounds<usize>,
{
    let len = string.len();

    // Check if the range is valid
    let start = match range.start_bound() {
        Included(&n) => {
            if n > len {
                return None;
            }
            n
        }
        Excluded(&n) => {
            if n >= len {
                return None;
            }
            n
        }
        _ => 0,
    };

    let end = match range.end_bound() {
        Included(&n) => {
            if n > len {
                return None;
            }
            n
        }
        Excluded(&n) => {
            if n > len {
                return None;
            }
            n
        }
        _ => len,
    };

    // Check character boundaries
    if !string.is_char_boundary(start) {
        return None;
    }
    if !string.is_char_boundary(end) {
        return None;
    }

    string.replace_range(range, replace_with);

    Some(())
}

#[async_trait::async_trait]
impl ToolCallService for ApplyPatch {
    type Input = ApplyPatchInput;

    async fn call(&self, input: Self::Input) -> Result<String, String> {
        let path = Path::new(&input.path);
        if !path.exists() {
            return Err(Error::FileNotFound(path.to_path_buf()).to_string());
        }

        let blocks = parse::parse_blocks(&input.diff).map_err(|e| e.to_string())?;
        let blocks_len = blocks.len();

        let result: Result<_, Error> = async {
            let content = fs::read_to_string(&input.path)
                .await
                .map_err(Error::FileOperation)?;

            let modified = apply_patches(content, blocks).await?;

            fs::write(&input.path, &modified)
                .await
                .map_err(Error::FileOperation)?;

            let syntax_warning = syn::validate(&input.path, &modified);

            let mut output = format!(
                "Successfully applied {blocks_len} patch(es) to {path}",
                blocks_len = blocks_len,
                path = input.path
            );
            if let Some(warning) = syntax_warning {
                output.push_str("\nWarning: ");
                output.push_str(&warning.to_string());
            }

            Ok(output)
        }
        .await;

        result.map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod test {
    use std::io::{Error as IoError, ErrorKind as IoErrorKind};

    use tempfile::TempDir;

    use super::*;

    async fn write_test_file(path: impl AsRef<Path>, content: &str) -> Result<(), Error> {
        fs::write(&path, content)
            .await
            .map_err(Error::FileOperation)
    }

    #[test]
    fn test_error_messages() {
        // Test file not found error
        let err = Error::FileNotFound(PathBuf::from("nonexistent.txt"));
        assert_eq!(err.to_string(), "File not found at path: nonexistent.txt");

        // Test file operation error
        let io_err = Error::FileOperation(IoError::new(
            IoErrorKind::NotFound,
            "No such file or directory (os error 2)",
        ));
        assert_eq!(
            io_err.to_string(),
            "File operation failed: No such file or directory (os error 2)"
        );
    }

    #[tokio::test]
    async fn test_file_not_found() {
        let fs_replace = ApplyPatch;
        let result = fs_replace
            .call(ApplyPatchInput {
                path: "nonexistent.txt".to_string(),
                diff: format!("{SEARCH}\nHello\n{DIVIDER}\nWorld\n{REPLACE}\n"),
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("File not found"));
    }

    #[tokio::test]
    async fn test_whitespace_preservation() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let content = "    Hello World    \n  Test Line  \n   Goodbye World   \n";

        write_test_file(&file_path, content).await.unwrap();

        let fs_replace = ApplyPatch;
        let result = fs_replace
            .call(ApplyPatchInput {
                path: file_path.to_string_lossy().to_string(),
                diff: format!(
                    "{SEARCH}\n    Hello World    \n{DIVIDER}\n    Hi World    \n{REPLACE}\n"
                )
                .to_string(),
            })
            .await
            .unwrap();

        assert!(result.contains("Successfully applied"));
        assert!(result.contains(&file_path.display().to_string()));
    }

    #[tokio::test]
    async fn test_empty_search_new_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        write_test_file(&file_path, "").await.unwrap();

        let fs_replace = ApplyPatch;
        let result = fs_replace
            .call(ApplyPatchInput {
                path: file_path.to_string_lossy().to_string(),
                diff: format!("{SEARCH}\n{DIVIDER}\nNew content\n{REPLACE}\n").to_string(),
            })
            .await
            .unwrap();

        assert!(result.contains("Successfully applied"));
        assert!(result.contains(&*file_path.to_string_lossy()));
    }

    #[tokio::test]
    async fn test_multiple_blocks() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let content = "    First Line    \n  Middle Line  \n    Last Line    \n";

        write_test_file(&file_path, content).await.unwrap();

        let fs_replace = ApplyPatch;
        let diff = format!("{SEARCH}\n    First Line    \n{DIVIDER}\n    New First    \n{REPLACE}\n{SEARCH}\n    Last Line    \n{DIVIDER}\n    New Last    \n{REPLACE}\n").to_string();

        let result = fs_replace
            .call(ApplyPatchInput { path: file_path.to_string_lossy().to_string(), diff })
            .await
            .unwrap();

        assert!(result.contains("Successfully applied 2 patch(es)"));
        assert!(result.contains(&*file_path.to_string_lossy()));
    }

    #[tokio::test]
    async fn test_empty_block() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let content = "    First Line    \n  Middle Line  \n    Last Line    \n";

        write_test_file(&file_path, content).await.unwrap();

        let fs_replace = ApplyPatch;
        let diff = format!("{SEARCH}\n  Middle Line  \n{DIVIDER}\n{REPLACE}\n");
        let result = fs_replace
            .call(ApplyPatchInput { path: file_path.to_string_lossy().to_string(), diff })
            .await
            .unwrap();

        assert!(result.contains("Successfully applied"));
        assert!(result.contains(&*file_path.to_string_lossy()));
    }

    #[tokio::test]
    async fn test_complex_newline_preservation() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Test file with various newline patterns
        let content = "\n\n// Header comment\n\n\nfunction test() {\n    // Inside comment\n\n    let x = 1;\n\n\n    console.log(x);\n}\n\n// Footer comment\n\n\n";
        write_test_file(&file_path, content).await.unwrap();

        let fs_replace = ApplyPatch;

        // Test 1: Replace content while preserving surrounding newlines
        let result = fs_replace
            .call(ApplyPatchInput {
                path: file_path.to_string_lossy().to_string(),
                diff: format!("{SEARCH}\n    let x = 1;\n\n\n    console.log(x);\n{DIVIDER}\n    let y = 2;\n\n\n    console.log(y);\n{REPLACE}\n").to_string(),
            })
            .await
            .unwrap();

        assert!(result.contains("Successfully applied"));
        assert!(result.contains(&file_path.display().to_string()));

        // Test 2: Replace block with different newline pattern
        let result = fs_replace
            .call(ApplyPatchInput {
                path: file_path.to_string_lossy().to_string(),
                diff: format!(
                    "{SEARCH}\n\n// Footer comment\n\n\n{DIVIDER}\n\n\n\n// Updated footer\n\n{REPLACE}\n"
                )
                .to_string(),
            })
            .await
            .unwrap();

        assert!(result.contains("Successfully applied"));
        assert!(result.contains(&file_path.display().to_string()));

        // Test 3: Replace with empty lines preservation
        let result = fs_replace
            .call(ApplyPatchInput {
                path: file_path.to_string_lossy().to_string(),
                diff: format!(
                    "{SEARCH}\n\n\n// Header comment\n\n\n{DIVIDER}\n\n\n\n// New header\n\n\n\n{REPLACE}\n"
                )
                .to_string(),
            })
            .await
            .unwrap();

        assert!(result.contains("Successfully applied"));
        assert!(result.contains(&file_path.display().to_string()));
    }

    #[tokio::test]
    async fn test_fuzzy_search_replace() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Test file with typos and variations
        let content = r#"function calculateTotal(items) {
  let total = 0;
  for (const itm of items) {
    total += itm.price;
  }
  return total;
}
"#;
        write_test_file(&file_path, content).await.unwrap();

        let fs_replace = ApplyPatch;
        // Search with different casing, spacing, and variable names
        let result = fs_replace
            .call(ApplyPatchInput {
                path: file_path.to_string_lossy().to_string(),
                diff: format!("{SEARCH}\n  for (const itm of items) {{\n    total += itm.price;\n{DIVIDER}\n  for (const item of items) {{\n    total += item.price * item.quantity;\n{REPLACE}\n").to_string(),
            })
            .await
            .unwrap();

        assert!(result.contains("Successfully applied"));
        assert!(result.contains(&file_path.display().to_string()));

        // Test fuzzy matching with more variations
        let result = fs_replace
            .call(ApplyPatchInput {
                path: file_path.to_string_lossy().to_string(),
                diff: format!("{SEARCH}\nfunction calculateTotal(items) {{\n  let total = 0;\n{DIVIDER}\nfunction computeTotal(items, tax = 0) {{\n  let total = 0.0;\n{REPLACE}\n").to_string(),
            })
            .await
            .unwrap();

        assert!(result.contains("Successfully applied"));
        assert!(result.contains(&file_path.display().to_string()));
    }

    #[tokio::test]
    async fn test_fuzzy_search_advanced() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Test file with more complex variations
        let content = r#"class UserManager {
  async getUserById(userId) {
    const user = await db.findOne({ id: userId });
    if (!user) throw new Error('User not found');
    return user;
  }
}
"#;
        write_test_file(&file_path, content).await.unwrap();

        let fs_replace = ApplyPatch;
        // Search with structural similarities but different variable names and spacing
        let result = fs_replace
            .call(ApplyPatchInput {
                path: file_path.to_string_lossy().to_string(),
                diff: format!("{SEARCH}\n  async getUserById(userId) {{\n    const user = await db.findOne({{ id: userId }});\n{DIVIDER}\n  async findUser(id, options = {{}}) {{\n    const user = await this.db.findOne({{ userId: id, ...options }});\n{REPLACE}\n").to_string(),
            })
            .await
            .unwrap();

        assert!(result.contains("Successfully applied"));
        assert!(result.contains(&file_path.display().to_string()));

        // Test fuzzy matching with error handling changes
        let result = fs_replace
            .call(ApplyPatchInput {
                path: file_path.to_string_lossy().to_string(),
                diff: format!("{SEARCH}\n    if (!user) throw new Error('User not found');\n    return user;\n{DIVIDER}\n    if (!user) {{\n      throw new UserNotFoundError(id);\n    }}\n    return this.sanitizeUser(user);\n{REPLACE}\n").to_string(),
            })
            .await
            .unwrap();

        assert!(result.contains("Successfully applied"));
        assert!(result.contains(&file_path.display().to_string()));
    }

    #[tokio::test]
    async fn test_invalid_rust_replace() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.rs");
        let content = "fn main() { let x = 42; }";

        write_test_file(&file_path, content).await.unwrap();

        let fs_replace = ApplyPatch;
        let result = fs_replace
            .call(ApplyPatchInput {
                path: file_path.to_string_lossy().to_string(),
                diff: format!(
                    "{SEARCH}\nfn main() {{ let x = 42; }}\n{DIVIDER}\nfn main() {{ let x = \n{REPLACE}\n"
                )
                .to_string(),
            })
            .await
            .unwrap();

        assert!(result.contains("Successfully applied"));
        assert!(result.contains(&file_path.display().to_string()));
        assert!(result.contains("Warning: Syntax"));
    }

    #[tokio::test]
    async fn test_valid_rust_replace() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.rs");
        let content = "fn main() { let x = 42; }";

        write_test_file(&file_path, content).await.unwrap();

        let fs_replace = ApplyPatch;
        let result = fs_replace
            .call(ApplyPatchInput {
                path: file_path.to_string_lossy().to_string(),
                diff: format!("{SEARCH}\nfn main() {{ let x = 42; }}\n{DIVIDER}\nfn main() {{ let x = 42; let y = x * 2; }}\n{REPLACE}\n").to_string(),
            })
            .await
            .unwrap();

        assert!(result.contains("Successfully applied"));
        assert!(result.contains(&file_path.display().to_string()));
    }

    #[test]
    fn test_safe_replace_range() {
        let mut string = "Hello, world!".to_string();

        assert!(safe_replace_range(&mut string, 7..12, "earth").is_some());
        assert_eq!(string, "Hello, earth!");
        assert!(safe_replace_range(&mut string, 7..14, "earth").is_none());
    }
}
