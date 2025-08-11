use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ReasoningDetail {
    pub r#type: String,
    pub text: Option<String>,
    pub signature: Option<String>,
}
