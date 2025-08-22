use base64::Engine;
use derive_getters::Getters;
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Debug, Serialize, Deserialize, Getters, PartialEq, Eq, Hash)]
pub struct Image {
    url: String,
    mime_type: String,
}

impl Image {
    pub fn new_bytes(content: Vec<u8>, mime_type: impl ToString) -> Self {
        let mime_type = mime_type.to_string();
        let base64_encoded = base64::engine::general_purpose::STANDARD.encode(&content);
        Self::new_base64(base64_encoded, mime_type)
    }

    // Returns the base64 image without the prefix.
    pub fn data(&self) -> &str {
        self.url
            .strip_prefix(&format!("data:{};base64,", self.mime_type))
            .unwrap_or(&self.url)
    }

    pub fn new_base64(base64_encoded: String, mime_type: impl ToString) -> Self {
        let mime_type = mime_type.to_string();
        let content = format!("data:{mime_type};base64,{base64_encoded}");
        Self { url: content, mime_type }
    }
}
