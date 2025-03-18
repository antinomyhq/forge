use forge_domain::{
    Error, ToolCallFull, ToolCallId, ToolCallPart, ToolName,
};

#[test]
fn test_fragmented_tool_call_recovery() {
    // This test verifies the fix for issue #541 where the agent fails to handle
    // fragmented tool call arguments across multiple parts

    // Simulate the exact error case from the reported issue
    let parts = [
        ToolCallPart {
            call_id: Some(ToolCallId::new("toolu_vrtx_01VibciqALXHDEjsKRfWXTRt")),
            name: Some(ToolName::new("tool_forge_fs_create")),
            arguments_part: "".to_string(),
        },
        ToolCallPart {
            call_id: None,
            name: None,
            arguments_part: "{\"path\": \"".to_string(),
        },
        ToolCallPart {
            call_id: None,
            name: None,
            arguments_part: "/Users/ami".to_string(),
        },
        ToolCallPart {
            call_id: None,
            name: None,
            arguments_part: "t/code-forg".to_string(),
        },
        ToolCallPart {
            call_id: None,
            name: None,
            arguments_part: "e/crates".to_string(),
        },
        ToolCallPart {
            call_id: None,
            name: None,
            arguments_part: "/forg".to_string(),
        },
        ToolCallPart {
            call_id: None,
            name: None,
            arguments_part: "e_ci/test".to_string(),
        },
        ToolCallPart {
            call_id: None,
            name: None,
            arguments_part: "s/c".to_string(),
        },
        ToolCallPart {
            call_id: None,
            name: None,
            arguments_part: "i.rs\",".to_string(),
        },
        ToolCallPart {
            call_id: None,
            name: None,
            arguments_part: "\"content\": \"test content\"}".to_string(),
        },
    ];

    // Our solution should now handle this correctly
    let result = ToolCallFull::try_from_parts(&parts);
    assert!(result.is_ok(), "Failed to parse fragmented JSON: {:?}", result.err());

    // Validate the parsed result
    let tool_calls = result.unwrap();
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].name.as_str(), "tool_forge_fs_create");
    
    // Validate the arguments were reconstructed correctly
    if let serde_json::Value::Object(map) = &tool_calls[0].arguments {
        assert_eq!(map["path"].as_str().unwrap(), "/Users/amit/code-forge/crates/forge_ci/tests/ci.rs");
        assert_eq!(map["content"].as_str().unwrap(), "test content");
    } else {
        panic!("Arguments are not a valid object");
    }
}

#[test]
fn test_error_handling_for_invalid_json() {
    // This test verifies the error handling for invalid JSON fragments
    let parts = [
        ToolCallPart {
            call_id: Some(ToolCallId::new("call_1")),
            name: Some(ToolName::new("tool_forge_fs_create")),
            arguments_part: "{\"path\": \"test.txt\",".to_string(), // Invalid JSON (missing closing brace)
        },
    ];

    let result = ToolCallFull::try_from_parts(&parts);
    
    // Should return an error but not panic
    assert!(result.is_err());
    match result {
        Err(Error::ToolCallFragmentParse { error: _, fragments }) => {
            assert_eq!(fragments, "{\"path\": \"test.txt\",");
        },
        _ => panic!("Expected ToolCallFragmentParse error, got: {:?}", result),
    }
}