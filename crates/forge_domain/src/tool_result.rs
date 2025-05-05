use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use gray_matter::{Matter, engine::YAML, ParsedEntity, Pod};
use serde_json::Value;

use crate::{ToolCallFull, ToolCallId, ToolName};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Setters)]
#[setters(strip_option, into)]
pub struct StandardizedToolResponse {
    pub name: ToolName,
    pub call_id: Option<ToolCallId>,
    pub content: String,
    pub is_error: bool,
    pub metadata: Option<Value>,
}

impl StandardizedToolResponse {
    pub fn new(name: ToolName) -> Self {
        Self {
            name,
            call_id: None,
            content: String::default(),
            is_error: false,
            metadata: None,
        }
    }

    pub fn to_frontmatter(&self) -> String {
        let mut frontmatter = String::new();
        frontmatter.push_str("---\n");
        frontmatter.push_str(&format!("tool: {}\n", self.name.as_str()));
        if let Some(call_id) = &self.call_id {
            frontmatter.push_str(&format!("call_id: '{}'\n", call_id));
        }
        frontmatter.push_str(&format!("is_error: {}\n", self.is_error));
        if let Some(metadata) = &self.metadata {
            let yaml = serde_yaml::to_string(metadata)
                .unwrap_or_else(|_| "{}".to_string());
            let yaml_content = yaml
                .trim_start_matches("---\n")
                .trim_end_matches("\n...")
                .lines()
                .map(|line| format!("  {}", line))
                .collect::<Vec<_>>()
                .join("\n");
            frontmatter.push_str("metadata:\n");
            frontmatter.push_str(&yaml_content);
            frontmatter.push('\n');
        }
        frontmatter.push_str("---\n");
        frontmatter.push_str(&self.content);
        frontmatter
    }

    pub fn from_frontmatter(input: &str) -> anyhow::Result<Self> {
        let matter = Matter::<YAML>::new();
        let parsed: ParsedEntity = matter.parse(input);
        
        let data = parsed.data.ok_or_else(|| anyhow::anyhow!("No front matter found"))?;
        
        let name = data["tool"]
            .as_string()
            .map_err(|_| anyhow::anyhow!("Invalid tool name"))?;
        let name = ToolName::new(name);

        let call_id = match data.as_hashmap() {
            Ok(hash) => {
                if let Some(id) = hash.get("call_id") {
                    match id.as_string() {
                        Ok(id) => Some(ToolCallId::new(id)),
                        Err(_) => None,
                    }
                } else {
                    None
                }
            }
            Err(_) => None,
        };

        let is_error = data["is_error"].as_bool().unwrap_or(false);

        // Try to get metadata by converting Pod to JSON Value
        let metadata = match data.as_hashmap() {
            Ok(hash) => {
                if let Some(meta) = hash.get("metadata") {
                    match meta {
                        Pod::Null => None,
                        pod => {
                            let json_value: Value = pod.clone().into();
                            Some(json_value)
                        }
                    }
                } else {
                    None
                }
            }
            Err(_) => None,
        };
        
        Ok(Self {
            name,
            call_id,
            content: parsed.content,
            is_error,
            metadata,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Setters)]
#[setters(strip_option, into)]
pub struct ToolResult {
    pub name: ToolName,
    pub call_id: Option<ToolCallId>,
    #[setters(skip)]
    pub content: String,
    #[setters(skip)]
    pub is_error: bool,
}

impl ToolResult {
    pub fn new(name: ToolName) -> ToolResult {
        Self {
            name,
            call_id: None,
            content: String::default(),
            is_error: false,
        }
    }

    pub fn success(mut self, content: impl Into<String>) -> Self {
        self.content = content.into();
        self.is_error = false;
        self
    }

    pub fn failure(mut self, err: anyhow::Error) -> Self {
        let mut output = String::new();
        output.push_str("\nERROR:\n");

        for cause in err.chain() {
            output.push_str(&format!("Caused by: {cause}\n"));
        }

        self.content = output;
        self.is_error = true;
        self
    }

    pub fn to_standardized(&self) -> StandardizedToolResponse {
        StandardizedToolResponse {
            name: self.name.clone(),
            call_id: self.call_id.clone(),
            content: self.content.clone(),
            is_error: self.is_error,
            metadata: None,
        }
    }

    pub fn from_standardized(standardized: StandardizedToolResponse) -> Self {
        Self {
            name: standardized.name,
            call_id: standardized.call_id,
            content: standardized.content,
            is_error: standardized.is_error,
        }
    }

    pub fn to_frontmatter(&self) -> String {
        self.to_standardized().to_frontmatter()
    }

    pub fn from_frontmatter(input: &str) -> anyhow::Result<Self> {
        StandardizedToolResponse::from_frontmatter(input).map(Self::from_standardized)
    }
}

impl From<ToolCallFull> for ToolResult {
    fn from(value: ToolCallFull) -> Self {
        Self {
            name: value.name,
            call_id: value.call_id,
            content: String::default(),
            is_error: false,
        }
    }
}

impl std::fmt::Display for ToolResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_frontmatter())
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_snapshot_minimal() {
        let result = ToolResult::new(ToolName::new("test_tool"));
        assert_snapshot!(result.to_string());
    }

    #[test]
    fn test_snapshot_full() {
        let result = ToolResult::new(ToolName::new("complex_tool"))
            .call_id(ToolCallId::new("123"))
            .failure(anyhow::anyhow!(
                json!({"key": "value", "number": 42}).to_string()
            ));
        assert_snapshot!(result.to_string());
    }

    #[test]
    fn test_snapshot_with_special_chars() {
        let result = ToolResult::new(ToolName::new("xml_tool")).success(
            json!({
                "text": "Special chars: < > & ' \"",
                "nested": {
                    "html": "<div>Test</div>"
                }
            })
            .to_string(),
        );
        assert_snapshot!(result.to_string());
    }

    #[test]
    fn test_display_minimal() {
        let result = ToolResult::new(ToolName::new("test_tool"));
        assert_snapshot!(result.to_string());
    }

    #[test]
    fn test_display_full() {
        let result = ToolResult::new(ToolName::new("complex_tool"))
            .call_id(ToolCallId::new("123"))
            .success(
                json!({
                    "user": "John Doe",
                    "age": 42,
                    "address": [{"city": "New York"}, {"city": "Los Angeles"}]
                })
                .to_string(),
            );
        assert_snapshot!(result.to_string());
    }

    #[test]
    fn test_display_special_chars() {
        let result = ToolResult::new(ToolName::new("xml_tool")).success(
            json!({
                "text": "Special chars: < > & ' \"",
                "nested": {
                    "html": "<div>Test</div>"
                }
            })
            .to_string(),
        );
        assert_snapshot!(result.to_string());
    }

    #[test]
    fn test_success_and_failure_content() {
        let success = ToolResult::new(ToolName::new("test_tool")).success("success message");
        assert!(!success.is_error);
        assert_eq!(success.content, "success message");

        let failure =
            ToolResult::new(ToolName::new("test_tool")).failure(anyhow::anyhow!("error message"));
        assert!(failure.is_error);
        assert_eq!(failure.content, "\nERROR:\nCaused by: error message\n");
    }

    #[test]
    fn test_standardized_roundtrip() {
        let original = ToolResult::new(ToolName::new("test_tool"))
            .call_id(ToolCallId::new("123"))
            .success("test content");
        
        let standardized = original.to_standardized();
        let frontmatter = standardized.to_frontmatter();
        let parsed = StandardizedToolResponse::from_frontmatter(&frontmatter).unwrap();
        let result = ToolResult::from_standardized(parsed);
        
        assert_eq!(original, result);
    }

    #[test]
    fn test_standardized_with_metadata() {
        let metadata = json!({
            "timestamp": "2024-03-20T12:00:00Z",
            "version": "1.0.0",
            "tags": ["test", "metadata"]
        });

        let standardized = StandardizedToolResponse {
            name: ToolName::new("test_tool"),
            call_id: Some(ToolCallId::new("123")),
            content: "Test content with metadata".to_string(),
            is_error: false,
            metadata: Some(metadata),
        };

        let frontmatter = standardized.to_frontmatter();
        let parsed = StandardizedToolResponse::from_frontmatter(&frontmatter).unwrap();
        
        assert_eq!(standardized, parsed);
        assert_snapshot!(frontmatter);
    }

    #[test]
    fn test_standardized_error_handling() {
        let standardized = StandardizedToolResponse {
            name: ToolName::new("error_tool"),
            call_id: None,
            content: "Error occurred during execution".to_string(),
            is_error: true,
            metadata: None,
        };

        let frontmatter = standardized.to_frontmatter();
        let parsed = StandardizedToolResponse::from_frontmatter(&frontmatter).unwrap();
        
        assert_eq!(standardized, parsed);
        assert_snapshot!(frontmatter);
    }

    #[test]
    fn test_standardized_complex_content() {
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

        let standardized = StandardizedToolResponse {
            name: ToolName::new("complex_tool"),
            call_id: Some(ToolCallId::new("456")),
            content: content.to_string(),
            is_error: false,
            metadata: Some(json!({
                "content_type": "markdown",
                "line_count": 15
            })),
        };

        let frontmatter = standardized.to_frontmatter();
        let parsed = StandardizedToolResponse::from_frontmatter(&frontmatter).unwrap();
        
        assert_eq!(standardized, parsed);
        assert_snapshot!(frontmatter);
    }

    #[test]
    fn test_standardized_invalid_frontmatter() {
        let invalid_input = "This is not a valid frontmatter document";
        let result = StandardizedToolResponse::from_frontmatter(invalid_input);
        assert!(result.is_err());
    }

    #[test]
    fn test_standardized_missing_required_fields() {
        let invalid_input = r#"---
metadata: {"key": "value"}
---
content"#;
        let result = StandardizedToolResponse::from_frontmatter(invalid_input);
        assert!(result.is_err());
    }

    #[test]
    fn test_standardized_null_metadata() {
        let standardized = StandardizedToolResponse {
            name: ToolName::new("test_tool"),
            call_id: Some(ToolCallId::new("123")),
            content: "Test content".to_string(),
            is_error: false,
            metadata: Some(json!(null)),
        };

        let frontmatter = standardized.to_frontmatter();
        let parsed = StandardizedToolResponse::from_frontmatter(&frontmatter).unwrap();
        
        assert_eq!(parsed.metadata, None);
    }

    #[test]
    fn test_standardized_invalid_metadata() {
        let standardized = StandardizedToolResponse {
            name: ToolName::new("test_tool"),
            call_id: Some(ToolCallId::new("123")),
            content: "Test content".to_string(),
            is_error: false,
            metadata: Some(json!({
                "invalid": std::f64::NAN,
                "nested": {
                    "invalid": std::f64::INFINITY
                }
            })),
        };

        let frontmatter = standardized.to_frontmatter();
        let parsed = StandardizedToolResponse::from_frontmatter(&frontmatter).unwrap();
        
        // Should handle invalid JSON values gracefully
        assert!(parsed.metadata.is_some());
    }

    #[test]
    fn test_standardized_empty_metadata() {
        let standardized = StandardizedToolResponse {
            name: ToolName::new("test_tool"),
            call_id: Some(ToolCallId::new("123")),
            content: "Test content".to_string(),
            is_error: false,
            metadata: Some(json!({})),
        };

        let frontmatter = standardized.to_frontmatter();
        let parsed = StandardizedToolResponse::from_frontmatter(&frontmatter).unwrap();
        
        assert_eq!(parsed.metadata, Some(json!({})));
    }

    #[test]
    fn test_standardized_malformed_frontmatter() {
        let invalid_inputs = vec![
            "---\nname: test_tool\n---",  // Missing content
            "---\n---",  // Missing required fields
            "---\nname: 123\n---",  // Invalid name type
            "---\nname: test_tool\nis_error: invalid\n---",  // Invalid is_error type
        ];

        for input in invalid_inputs {
            let result = StandardizedToolResponse::from_frontmatter(input);
            assert!(result.is_err(), "Should fail for input: {}", input);
        }
    }
}
