use forge_domain::ExecutableTool;
use insta::assert_snapshot;
use tokio::fs;

use crate::outline::{Outline, OutlineInput};
use crate::tools::utils::TempDir;

#[tokio::test]
async fn python_outline() {
    let temp_dir = TempDir::new().unwrap();
    let content = r#"
def greet(name: str) -> str:
    return f"Hello, {name}!"

# Class with inheritance
class Person:
    def __init__(self, name: str):
        self.name = name

    def say_hello(self):
        return greet(self.name)

# Decorated method
def decorator(func):
    def wrapper(*args, **kwargs):
        return func(*args, **kwargs)
    return wrapper

@decorator
def decorated_function():
    pass

# Async function
async def fetch_data():
    return "data""#;
    let file_path = temp_dir.path().join("test.py");
    fs::write(&file_path, content).await.unwrap();

    let outline = Outline;
    let result = outline
        .call(OutlineInput { path: temp_dir.path().to_string_lossy().to_string() })
        .await
        .unwrap();

    assert_snapshot!("outline_python", result);
}
