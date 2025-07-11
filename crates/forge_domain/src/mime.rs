use std::fmt::Display;

#[derive(Debug, Clone, PartialEq)]
pub enum MimeType {
    Text,
    Image(String),
    Pdf,
    Other(String),
}

impl From<&str> for MimeType {
    fn from(value: &str) -> Self {
        if value.starts_with("text") {
            return MimeType::Text;
        }

        if let Some(image) = value.strip_prefix("image/") {
            return MimeType::Image(image.to_string());
        }

        if value == "application/pdf" {
            return MimeType::Pdf;
        }

        MimeType::Other(value.to_string())
    }
}

impl Display for MimeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MimeType::Text => write!(f, "text"),
            MimeType::Image(image) => write!(f, "image/{image}"),
            MimeType::Pdf => write!(f, "application/pdf"),
            MimeType::Other(other) => write!(f, "{other}"),
        }
    }
}
