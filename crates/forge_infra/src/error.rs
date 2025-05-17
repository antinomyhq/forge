#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Unsupported MCP response: {0}")]
    UnsupportedMcpResponse(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;
