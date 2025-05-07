use chrono::Utc;
use derive_setters::Setters;
use gray_matter::{engine::YAML, Matter};
use serde::{Deserialize, Serialize};
use serde_yml;

use crate::{ResponseContent, ToolCallId, ToolName};

/// Standardized response format for all tool responses
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Setters)]
#[setters(strip_option, into)]
pub struct ToolResponse {
    /// The name of the tool that generated this response
    pub tool_name: ToolName,
    /// Optional call ID for tracking tool calls
    pub call_id: Option<ToolCallId>,
    /// The actual response content
    #[setters(skip)]
    pub content: ResponseContent,
    /// Metadata about the response
    #[setters(skip)]
    pub metadata: ResponseMetadata,
}

/// Metadata about the tool response
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResponseMetadata {
    /// Timestamp of when the response was generated
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Version of the response format
    pub version: String,
}

impl ToolResponse {
    /// Create a new successful tool response
    pub fn success(tool_name: ToolName, content: impl Into<String>) -> Self {
        Self {
            tool_name,
            call_id: None,
            content: ResponseContent::Success(content.into()),
            metadata: ResponseMetadata {
                timestamp: Utc::now(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        }
    }

    /// Create a new error tool response
    pub fn error(tool_name: ToolName, content: impl Into<String>) -> Self {
        let content = content.into();
        let error_content = if !content.starts_with("ERROR:") {
            format!("ERROR: {}", content)
        } else {
            content
        };
        Self {
            tool_name,
            call_id: None,
            content: ResponseContent::Error(error_content),
            metadata: ResponseMetadata {
                timestamp: Utc::now(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        }
    }

    /// Convert the response to a frontmatter-formatted string
    pub fn to_frontmatter(&self) -> String {
        let yaml = serde_yml::to_string(self).unwrap_or_default();
        format!("---\n{}\n---\n{}", yaml, self.content.to_string())
    }

    /// Parse a frontmatter-formatted string into a ToolResponse
    pub fn from_frontmatter(input: &str) -> anyhow::Result<Self> {
        let matter = Matter::<YAML>::new();
        let result = matter.parse(input);
        let yaml_str = result.data.map(|v| format!("{:?}", v)).unwrap_or_default();
        let yaml_str = yaml_str.trim_start_matches("Object(").trim_end_matches(")");
        let yaml_str = yaml_str.replace(", ", "\n");
        let mut response: ToolResponse = serde_yml::from_str(&yaml_str)?;
        response.content = ResponseContent::from(result.content.as_str());
        Ok(response)
    }
}

impl std::fmt::Display for ToolResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_frontmatter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolName;

    #[test]
    fn test_success_response() {
        let response = ToolResponse::success(
            ToolName::new("test_tool"),
            "Success message"
        );
        
        let frontmatter = response.to_frontmatter();
        println!("Success Response Frontmatter:\n{}", frontmatter);
        
        let parsed = ToolResponse::from_frontmatter(&frontmatter).unwrap();
        assert_eq!(response.tool_name, parsed.tool_name);
        assert!(matches!(parsed.content, ResponseContent::Success(_)));
    }

    #[test]
    fn test_error_response() {
        let response = ToolResponse::error(
            ToolName::new("test_tool"),
            "ERROR: Something went wrong"
        );
        
        let frontmatter = response.to_frontmatter();
        println!("\nError Response Frontmatter:\n{}", frontmatter);
        
        let parsed = ToolResponse::from_frontmatter(&frontmatter).unwrap();
        assert_eq!(response.tool_name, parsed.tool_name);
        assert!(matches!(parsed.content, ResponseContent::Error(_)));
    }
} 