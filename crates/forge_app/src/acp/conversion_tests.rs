//! Tests for ACP conversion to verify markdown is sent for display, not XML

use forge_domain::{FileDiff, Image, ToolOutput, ToolValue};

use super::conversion::ToolOutputConverter;

#[test]
fn test_markdown_sent_to_acp_not_xml() {
    // Create a paired output with XML for LLM and Markdown for display
    let xml = "<file>test content</file>";
    let markdown = "## File: test.txt\n\nContent here";
    
    let output = ToolOutput::paired(xml, markdown);
    
    // Convert to ACP content
    let acp_content = ToolOutputConverter::convert(&output);
    
    // Should have exactly one content item
    assert_eq!(acp_content.len(), 1, "Should have one content item");
    
    // Extract the text content
    if let Some(agent_client_protocol::ToolCallContent::Content(content)) = acp_content.first() {
        if let agent_client_protocol::ContentBlock::Text(text) = &content.content {
            // Verify it's markdown, not XML
            assert_eq!(text.text, markdown, "Should send markdown to ACP");
            assert!(!text.text.contains("<file>"), "Should not contain XML tags");
            assert!(text.text.contains("## File:"), "Should contain markdown header");
        } else {
            panic!("Expected text content block");
        }
    } else {
        panic!("Expected content, got: {:?}", acp_content);
    }
}

#[test]
fn test_plain_markdown_sent_to_acp() {
    // Create output with just markdown (no pair)
    let markdown = "## Result\n\nOperation completed successfully";
    let output = ToolOutput::markdown(markdown);
    
    // Convert to ACP content
    let acp_content = ToolOutputConverter::convert(&output);
    
    // Should have exactly one content item
    assert_eq!(acp_content.len(), 1);
    
    // Verify markdown is sent
    if let Some(agent_client_protocol::ToolCallContent::Content(content)) = acp_content.first() {
        if let agent_client_protocol::ContentBlock::Text(text) = &content.content {
            assert_eq!(text.text, markdown);
            assert!(text.text.contains("## Result"));
        } else {
            panic!("Expected text content block");
        }
    } else {
        panic!("Expected content");
    }
}

#[test]
fn test_file_diff_sent_to_acp() {
    // Create a paired output with XML and FileDiff
    let xml = "<file_diff path=\"test.txt\">diff content</file_diff>";
    let file_diff = FileDiff {
        path: "test.txt".to_string(),
        old_text: Some("old content".to_string()),
        new_text: "new content".to_string(),
    };
    
    let output = ToolOutput {
        is_error: false,
        values: vec![ToolValue::Pair(
            Box::new(ToolValue::Text(xml.to_string())),
            Box::new(ToolValue::FileDiff(file_diff.clone())),
        )],
    };
    
    // Convert to ACP content
    let acp_content = ToolOutputConverter::convert(&output);
    
    // Should have exactly one diff item
    assert_eq!(acp_content.len(), 1);
    
    // Verify FileDiff is sent
    if let Some(agent_client_protocol::ToolCallContent::Diff(diff)) = acp_content.first() {
        assert_eq!(diff.path.to_str().unwrap(), "test.txt");
        assert_eq!(diff.new_text, "new content");
        assert_eq!(diff.old_text.as_deref(), Some("old content"));
    } else {
        panic!("Expected diff content, got: {:?}", acp_content);
    }
}

#[test]
fn test_empty_markdown_filtered() {
    // Create output with empty markdown
    let output = ToolOutput::markdown("");
    
    // Convert to ACP content
    let acp_content = ToolOutputConverter::convert(&output);
    
    // Should have no content (empty markdown is filtered)
    assert_eq!(acp_content.len(), 0, "Empty markdown should be filtered out");
}

#[test]
fn test_image_sent_to_acp() {
    // Create output with an image
    let image_data = vec![1, 2, 3, 4];
    let image = Image::new_bytes(image_data.clone(), "image/png".to_string());
    let output = ToolOutput::image(image.clone());
    
    // Convert to ACP content
    let acp_content = ToolOutputConverter::convert(&output);
    
    // Should have exactly one image item
    assert_eq!(acp_content.len(), 1);
    
    // Verify image is sent
    if let Some(agent_client_protocol::ToolCallContent::Content(content)) = acp_content.first() {
        if let agent_client_protocol::ContentBlock::Image(img) = &content.content {
            // Image data is base64 encoded
            assert_eq!(img.data, image.data());
            assert_eq!(img.mime_type, "image/png");
        } else {
            panic!("Expected image content block");
        }
    } else {
        panic!("Expected content");
    }
}

#[test]
fn test_multiple_values_with_markdown() {
    // Create output with multiple values including paired XML/Markdown
    let xml1 = "<result>Result 1</result>";
    let md1 = "## Result 1\n\nFirst result";
    let xml2 = "<result>Result 2</result>";
    let md2 = "## Result 2\n\nSecond result";
    
    let output = ToolOutput {
        is_error: false,
        values: vec![
            ToolValue::Pair(
                Box::new(ToolValue::Text(xml1.to_string())),
                Box::new(ToolValue::Markdown(md1.to_string())),
            ),
            ToolValue::Pair(
                Box::new(ToolValue::Text(xml2.to_string())),
                Box::new(ToolValue::Markdown(md2.to_string())),
            ),
        ],
    };
    
    // Convert to ACP content
    let acp_content = ToolOutputConverter::convert(&output);
    
    // Should have two content items
    assert_eq!(acp_content.len(), 2, "Should have two markdown items");
    
    // Verify both are markdown, not XML
    for (idx, content) in acp_content.iter().enumerate() {
        if let agent_client_protocol::ToolCallContent::Content(content) = content {
            if let agent_client_protocol::ContentBlock::Text(text) = &content.content {
                assert!(text.text.contains("## Result"), "Should contain markdown header");
                assert!(!text.text.contains("<result>"), "Should not contain XML tags");
            } else {
                panic!("Expected text content block at index {}", idx);
            }
        } else {
            panic!("Expected content at index {}", idx);
        }
    }
}

#[test]
fn test_text_skipped_when_file_diff_present() {
    // When FileDiff is present, plain text (CLI diff) should be skipped
    let cli_diff = "--- old\n+++ new\n-old line\n+new line";
    let file_diff = FileDiff {
        path: "test.txt".to_string(),
        old_text: Some("old content".to_string()),
        new_text: "new content".to_string(),
    };
    
    let output = ToolOutput {
        is_error: false,
        values: vec![
            ToolValue::Text(cli_diff.to_string()),
            ToolValue::FileDiff(file_diff),
        ],
    };
    
    // Convert to ACP content
    let acp_content = ToolOutputConverter::convert(&output);
    
    // Should have only the FileDiff, text should be filtered
    assert_eq!(acp_content.len(), 1, "Should only have FileDiff");
    
    // Verify it's a diff, not text
    assert!(
        matches!(acp_content.first(), Some(agent_client_protocol::ToolCallContent::Diff(_))),
        "Should be a Diff, not text"
    );
}
