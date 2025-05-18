use base64::Engine;
use derive_getters::Getters;
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};

#[derive(Clone, Debug, Serialize, Deserialize, Getters, PartialEq, Eq, Hash)]
pub struct Image {
    url: String,
    mime_type: MimeType,
}

#[derive(AsRefStr, EnumString, Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MimeType {
    #[strum(serialize = "png")]
    Png,
    #[strum(serialize = "jpg", serialize = "jpeg")]
    Jpeg,
}

impl Image {
    pub fn new_base64(content: Vec<u8>, mime_type: MimeType) -> Self {
        let base64_encoded = base64::engine::general_purpose::STANDARD.encode(&content);
        let mime_type_str = mime_type.as_ref();
        let content = format!("data:image/{mime_type_str};base64,{base64_encoded}");
        Self { url: content, mime_type }
    }
}
