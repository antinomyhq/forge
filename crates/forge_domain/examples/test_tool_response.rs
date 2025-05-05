use forge_domain::{StandardizedToolResponse, ToolName, ToolCallId};
use serde_json::json;

fn main() -> anyhow::Result<()> {
    // Test 1: Basic response
    println!("Test 1: Basic response");
    let response = StandardizedToolResponse {
        name: ToolName::new("test_tool"),
        call_id: Some(ToolCallId::new("123")),
        content: "This is a test response".to_string(),
        is_error: false,
        metadata: None,
    };
    let frontmatter = response.to_frontmatter();
    println!("Frontmatter output:\n{}", frontmatter);
    let parsed = StandardizedToolResponse::from_frontmatter(&frontmatter)?;
    println!("Parsed response matches original: {}", parsed == response);
    println!();

    // Test 2: Response with metadata
    println!("Test 2: Response with metadata");
    let response = StandardizedToolResponse {
        name: ToolName::new("metadata_tool"),
        call_id: Some(ToolCallId::new("456")),
        content: "Response with metadata".to_string(),
        is_error: false,
        metadata: Some(json!({
            "timestamp": "2024-03-20T12:00:00Z",
            "version": "1.0.0",
            "tags": ["test", "metadata"]
        })),
    };
    let frontmatter = response.to_frontmatter();
    println!("Frontmatter output:\n{}", frontmatter);
    let parsed = StandardizedToolResponse::from_frontmatter(&frontmatter)?;
    println!("Parsed response matches original: {}", parsed == response);
    println!();

    // Test 3: Error response
    println!("Test 3: Error response");
    let response = StandardizedToolResponse {
        name: ToolName::new("error_tool"),
        call_id: None,
        content: "An error occurred".to_string(),
        is_error: true,
        metadata: None,
    };
    let frontmatter = response.to_frontmatter();
    println!("Frontmatter output:\n{}", frontmatter);
    let parsed = StandardizedToolResponse::from_frontmatter(&frontmatter)?;
    println!("Parsed response matches original: {}", parsed == response);
    println!();

    // Test 4: Complex content
    println!("Test 4: Complex content");
    let content = r#"# Complex Content
This is a test with multiple lines
and special characters: < > & ' "

## Code Block
```rust
fn main() {
    println!("Hello, world!");
}
```

## List
- Item 1
- Item 2
- Item 3"#;
    let response = StandardizedToolResponse {
        name: ToolName::new("complex_tool"),
        call_id: Some(ToolCallId::new("789")),
        content: content.to_string(),
        is_error: false,
        metadata: Some(json!({
            "content_type": "markdown",
            "line_count": 15
        })),
    };
    let frontmatter = response.to_frontmatter();
    println!("Frontmatter output:\n{}", frontmatter);
    let parsed = StandardizedToolResponse::from_frontmatter(&frontmatter)?;
    println!("Parsed response matches original: {}", parsed == response);
    println!();

    // Test 5: Invalid frontmatter
    println!("Test 5: Invalid frontmatter");
    let invalid_input = "This is not a valid frontmatter document";
    let result = StandardizedToolResponse::from_frontmatter(invalid_input);
    println!("Error handling invalid input: {}", result.is_err());
    println!();

    Ok(())
} 