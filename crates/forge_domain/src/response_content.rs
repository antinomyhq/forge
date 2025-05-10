use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum ResponseContent {
    Success(String),
    Error(String),
}

impl ResponseContent {
    pub fn to_string(&self) -> String {
        match self {
            ResponseContent::Success(content) => content.clone(),
            ResponseContent::Error(content) => content.clone(),
        }
    }
}

impl From<&str> for ResponseContent {
    fn from(input: &str) -> Self {
        let content = input.trim();
        if content.starts_with("ERROR:") {
            ResponseContent::Error(content.to_string())
        } else {
            ResponseContent::Success(content.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_success_content() {
        let content = ResponseContent::Success("test content".to_string());
        assert_eq!(content.to_string(), "test content");
    }

    #[test]
    fn test_error_content() {
        let content = ResponseContent::Error("error message".to_string());
        assert_eq!(content.to_string(), "error message");
    }

    #[test]
    fn test_from_str_success() {
        let content = ResponseContent::from("test content");
        assert!(matches!(content, ResponseContent::Success(_)));
    }

    #[test]
    fn test_from_str_error() {
        let content = ResponseContent::from("ERROR: test error");
        assert!(matches!(content, ResponseContent::Error(_)));
    }
} 