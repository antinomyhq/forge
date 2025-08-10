use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, PartialEq, thiserror::Error)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Error {
    #[error("Overload error: {message}")]
    OverloadedError { message: String },
}
