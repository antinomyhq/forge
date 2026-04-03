use std::path::PathBuf;

use forge_domain::{
    ContextMessage, Conversation, NormalizeToolCallArguments, ToolCallArguments, Transformer,
};

/// Loads the kimi-k2p5-turbo error dump and verifies that tool call arguments
/// are properly normalized from strings to JSON objects.
#[tokio::test]
async fn test_kimi_k2p5_turbo_dump_normalization() {
    // Load the dump file
    let dump_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../kimi-k2p5-turbo-error-dump.json");
    
    let content = tokio::fs::read_to_string(&dump_path).await
        .expect("Failed to read kimi-k2p5-turbo-error-dump.json");
    
    // Parse as ConversationDump format
    #[derive(serde::Deserialize)]
    struct ConversationDump {
        conversation: Conversation,
    }
    
    let dump: ConversationDump = serde_json::from_str(&content)
        .expect("Failed to parse conversation dump");
    
    let conversation = dump.conversation;
    let context = conversation.context.expect("Conversation should have context");
    
    // Apply the normalization transform
    let mut transformer = NormalizeToolCallArguments::new();
    let normalized = transformer.transform(context);
    
    // Find all patch tool calls and verify their arguments are objects, not strings
    let mut patch_tool_calls_found = 0;
    let mut errors = Vec::new();
    
    for entry in &normalized.messages {
        if let ContextMessage::Text(text_msg) = &entry.message {
            if let Some(tool_calls) = &text_msg.tool_calls {
                for tool_call in tool_calls {
                    if tool_call.name.as_str() == "patch" {
                        patch_tool_calls_found += 1;
                        
                        // Check the arguments
                        match &tool_call.arguments {
                            ToolCallArguments::Parsed(value) => {
                                // Verify it's a JSON object, not a string
                                if !value.is_object() {
                                    errors.push(format!(
                                        "Patch tool call {} has Parsed arguments that are not an object: {:?}",
                                        tool_call.call_id.as_ref().map(|c| c.as_str()).unwrap_or("unknown"),
                                        value
                                    ));
                                }
                            }
                            ToolCallArguments::Unparsed(str) => {
                                errors.push(format!(
                                    "Patch tool call {} still has Unparsed arguments: {}",
                                    tool_call.call_id.as_ref().map(|c| c.as_str()).unwrap_or("unknown"),
                                    str
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Print diagnostics
    println!("Found {} patch tool calls", patch_tool_calls_found);
    
    if !errors.is_empty() {
        println!("Errors found:");
        for error in &errors {
            println!("  - {}", error);
        }
    }
    
    // We should have found at least 2 patch tool calls (from lines 1172 and 1177)
    assert!(patch_tool_calls_found >= 2, "Expected at least 2 patch tool calls, found {}", patch_tool_calls_found);
    
    // All patch tool calls should have properly normalized arguments
    assert!(errors.is_empty(), "Found {} patch tool calls with string arguments that weren't normalized:\n{}", 
        errors.len(),
        errors.join("\n")
    );
}

/// Test that serializing the normalized context produces JSON objects for arguments
#[tokio::test]
async fn test_kimi_k2p5_turbo_dump_serialization() {
    // Load the dump file
    let dump_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../kimi-k2p5-turbo-error-dump.json");
    
    let content = tokio::fs::read_to_string(&dump_path).await
        .expect("Failed to read kimi-k2p5-turbo-error-dump.json");
    
    #[derive(serde::Deserialize)]
    struct ConversationDump {
        conversation: Conversation,
    }
    
    let dump: ConversationDump = serde_json::from_str(&content)
        .expect("Failed to parse conversation dump");
    
    let conversation = dump.conversation;
    let context = conversation.context.expect("Conversation should have context");
    
    // Apply the normalization transform
    let mut transformer = NormalizeToolCallArguments::new();
    let normalized = transformer.transform(context);
    
    // Serialize to JSON
    let serialized = serde_json::to_string(&normalized).expect("Failed to serialize normalized context");
    
    // Parse back as generic JSON to inspect the structure
    let json_value: serde_json::Value = serde_json::from_str(&serialized).expect("Failed to parse serialized JSON");
    
    // Find all tool_calls and verify arguments are objects
    let mut errors = Vec::new();
    
    if let Some(messages) = json_value.get("messages").and_then(|m| m.as_array()) {
        for (msg_idx, msg) in messages.iter().enumerate() {
            if let Some(text) = msg.get("text") {
                if let Some(tool_calls) = text.get("tool_calls").and_then(|t| t.as_array()) {
                    for (tool_idx, tool_call) in tool_calls.iter().enumerate() {
                        if let Some(arguments) = tool_call.get("arguments") {
                            // Arguments should be an object, not a string
                            if arguments.is_string() {
                                let tool_name = tool_call.get("name")
                                    .and_then(|n| n.as_str())
                                    .unwrap_or("unknown");
                                errors.push(format!(
                                    "Message {} tool_call {} ({}): arguments is still a string: {}",
                                    msg_idx, tool_idx, tool_name, arguments.as_str().unwrap_or("")
                                ));
                            } else if !arguments.is_object() {
                                errors.push(format!(
                                    "Message {} tool_call {}: arguments is neither string nor object: {:?}",
                                    msg_idx, tool_idx, arguments
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    
    if !errors.is_empty() {
        println!("Serialization errors:");
        for error in &errors {
            println!("  - {}", error);
        }
    }
    
    assert!(errors.is_empty(), "Found {} tool calls with string arguments in serialized output:\n{}",
        errors.len(),
        errors.join("\n")
    );
}
