use forge_domain::{StandardizedToolResponse, ToolName, ToolCallId};
use serde_json::json;

fn main() -> anyhow::Result<()> {
    // Create a standardized response
    let response = StandardizedToolResponse {
        name: ToolName::new("example_tool"),
        call_id: Some(ToolCallId::new("123")),
        content: "This is an example tool response".to_string(),
        is_error: false,
        metadata: Some(json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "version": "1.0.0"
        })),
    };

    // Convert to frontmatter
    let frontmatter = response.to_frontmatter();
    println!("Frontmatter output:\n{}", frontmatter);

    // Parse back from frontmatter
    let parsed = StandardizedToolResponse::from_frontmatter(&frontmatter)?;
    println!("\nParsed response:");
    println!("Name: {:?}", parsed.name);
    println!("Call ID: {:?}", parsed.call_id);
    println!("Content: {}", parsed.content);
    println!("Is Error: {}", parsed.is_error);
    println!("Metadata: {:?}", parsed.metadata);

    Ok(())
} 