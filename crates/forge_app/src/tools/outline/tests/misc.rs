use forge_domain::ExecutableTool;
use insta::assert_snapshot;
use tokio::fs;

use crate::outline::{Outline, OutlineInput};
use crate::tools::utils::TempDir;

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

    let seperator = "\n|----\n";
    let mut result = result.split(seperator).collect::<Vec<_>>();
    result.sort();
    result = result.iter().map(|x| x.trim()).collect();
    let result = result.join(seperator);
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
