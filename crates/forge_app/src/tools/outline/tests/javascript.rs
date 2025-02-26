use forge_domain::ExecutableTool;
use insta::assert_snapshot;
use tokio::fs;

use crate::outline::{Outline, OutlineInput};
use crate::tools::utils::TempDir;

#[tokio::test]
async fn javascript_outline() {
    let temp_dir = TempDir::new().unwrap();
    let content = r#"
// Basic function
function calculateTotal(items) {
    return items.reduce((sum, item) => sum + item.price, 0);
}

// Arrow function
const processItems = (items) => {
    return items.map(item => item.name);
};

class ShoppingCart {
    constructor() {
        this.items = [];
    }

    // Instance method
    addItem(item) {
        this.items.push(item);
    }

    // Static method
    static getTotalPrice(items) {
        return calculateTotal(items);
    }
}

// Async function
async function fetchItems() {
    return Promise.resolve([]);
}"#;
    let file_path = temp_dir.path().join("test.js");
    fs::write(&file_path, content).await.unwrap();

    let outline = Outline;
    let result = outline
        .call(OutlineInput { path: temp_dir.path().to_string_lossy().to_string() })
        .await
        .unwrap();

    assert_snapshot!("outline_javascript", result);
}
