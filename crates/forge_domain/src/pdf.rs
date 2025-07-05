use base64::Engine;
use derive_getters::Getters;
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Debug, Serialize, Deserialize, Getters, PartialEq, Eq, Hash)]
pub struct Pdf {
    filename: String,
    file_data: String,
    // Anthropic needs just the base64 string, not the full data URL
    file_data_base64: String,
    mime_type: String,
}

impl Pdf {
    pub fn new_bytes(name: impl ToString, content: &[u8], mime_type: impl ToString) -> Self {
        let mime_type = mime_type.to_string();
        let base64_encoded = base64::engine::general_purpose::STANDARD.encode(content);
        Self::new_base64(name, base64_encoded, mime_type)
    }

    pub fn new_base64(
        name: impl ToString,
        base64_encoded: String,
        mime_type: impl ToString,
    ) -> Self {
        let mime_type = mime_type.to_string();
        let content = format!("data:{mime_type};base64,{base64_encoded}");
        Self {
            filename: name.to_string(),
            file_data: content,
            file_data_base64: base64_encoded,
            mime_type,
        }
    }
}
