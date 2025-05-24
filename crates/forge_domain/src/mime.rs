use std::fmt::Display;

#[derive(Debug, Clone, PartialEq)]
pub enum MimeType {
    Text,
    Image(String),
    Other(String),
}

impl From<&str> for MimeType {
    fn from(value: &str) -> Self {
        if value.starts_with("text") {
            MimeType::Text
        } else if let Some(image) = value.strip_prefix("image/") {
            MimeType::Image(image.to_string())
        } else {
            MimeType::Other(value.to_string())
        }
    }
}

impl Display for MimeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MimeType::Text => write!(f, "text"),
            MimeType::Image(image) => write!(f, "image/{image}"),
            MimeType::Other(other) => write!(f, "{other}"),
        }
    }
}
