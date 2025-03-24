use forge_domain::ExecutableTool;
use insta::assert_snapshot;
use tokio::fs;

use crate::outline::{Outline, OutlineInput};
use crate::tools::utils::TempDir;

#[tokio::test]
async fn rust_outline() {
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
