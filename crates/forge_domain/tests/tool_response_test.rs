use forge_domain::{ToolName, ToolResponse, ToolResult, ResponseContent};

#[test]
fn test_basic_tool_response() {
    // Test success case
    let response = ToolResponse::success(
        ToolName::new("test_tool"),
        "Operation completed successfully"
    );
    
    let frontmatter = response.to_frontmatter();
    println!("Success Response Frontmatter:\n{}", frontmatter);
    
    // Parse it back
    let parsed = ToolResponse::from_frontmatter(&frontmatter).unwrap();
    assert!(matches!(parsed.content, ResponseContent::Success(_)));
    
    // Test error case
    let error_response = ToolResponse::error(
        ToolName::new("test_tool"),
        "ERROR: Something went wrong"
    );
    
    let error_frontmatter = error_response.to_frontmatter();
    println!("\nError Response Frontmatter:\n{}", error_frontmatter);
    
    // Parse error back
    let parsed_error = ToolResponse::from_frontmatter(&error_frontmatter).unwrap();
    assert!(matches!(parsed_error.content, ResponseContent::Error(_)));
}

#[test]
fn test_tool_result_conversion() {
    // Create a ToolResult
    let result = ToolResult::new(ToolName::new("conversion_test"))
        .success("Test content");
    
    // Convert to ToolResponse
    let response = result.clone().into_response();
    let frontmatter = response.to_frontmatter();
    println!("\nConverted Response Frontmatter:\n{}", frontmatter);
    
    // Convert back to ToolResult
    let parsed_response = ToolResponse::from_frontmatter(&frontmatter).unwrap();
    let converted_result = ToolResult::from_response(parsed_response);
    
    assert_eq!(result.name, converted_result.name);
    assert_eq!(result.content, converted_result.content);
    assert_eq!(result.is_error, converted_result.is_error);
}

#[test]
fn test_complex_content() {
    // Test with complex JSON content
    let json_content = r#"{
        "user": "John Doe",
        "age": 42,
        "address": {
            "city": "New York",
            "country": "USA"
        },
        "tags": ["test", "example"]
    }"#;
    
    let response = ToolResponse::success(
        ToolName::new("json_tool"),
        json_content
    );
    
    let frontmatter = response.to_frontmatter();
    println!("\nComplex JSON Response Frontmatter:\n{}", frontmatter);
    
    // Parse and verify
    let parsed = ToolResponse::from_frontmatter(&frontmatter).unwrap();
    match parsed.content {
        ResponseContent::Success(content) => {
            assert_eq!(content.trim(), json_content.trim());
        }
        _ => panic!("Expected success content"),
    }
}

#[test]
fn test_error_chain() {
    // Test with a chain of errors
    let error = anyhow::anyhow!("Root error")
        .context("First context")
        .context("Second context");
    
    let result = ToolResult::new(ToolName::new("error_test"))
        .failure(error);
    
    let response = result.clone().into_response();
    let frontmatter = response.to_frontmatter();
    println!("\nError Chain Response Frontmatter:\n{}", frontmatter);
    
    // Parse and verify
    let parsed = ToolResponse::from_frontmatter(&frontmatter).unwrap();
    assert!(matches!(parsed.content, ResponseContent::Error(_)));
} 