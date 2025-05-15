use derive_more::derive::Display;
use thiserror::Error;

use super::response::ErrorResponse;

#[derive(Debug, Display, derive_more::From, Error)]
pub enum Error {
    EmptyContent,
    #[display("Upstream: {_0}")]
    Upstream(ErrorResponse),
    SerdeJson(serde_json::Error),
    ToolCallMissingName,
}
