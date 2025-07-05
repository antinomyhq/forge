use base64::Engine;
use derive_getters::Getters;
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Debug, Serialize, Deserialize, Getters, PartialEq, Eq, Hash)]
pub struct Image {
    url: String,
    mime_type: String,
}

impl Image {
    pub fn new_bytes(content: &[u8], mime_type: impl ToString) -> Self {
        let mime_type = mime_type.to_string();
        let base64_encoded = base64::engine::general_purpose::STANDARD.encode(content);
        Self::new_base64(base64_encoded, mime_type)
    }

    pub fn new_base64(base64_encoded: String, mime_type: impl ToString) -> Self {
        let mime_type = mime_type.to_string();
        let content = format!("data:{mime_type};base64,{base64_encoded}");
        Self { url: content, mime_type }
    }
}
