#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Unsupported MCP response: {0}")]
    UnsupportedMcpResponse(&'static str),
}
