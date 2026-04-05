use agent_client_protocol as acp;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("ACP protocol error: {0}")]
    Protocol(#[from] acp::Error),

    #[error("Forge application error: {0}")]
    Application(#[from] anyhow::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Converts a domain Error into an acp::Error.
///
/// AGENTS.md forbids blanket `From` impls for domain error conversion.
/// Call this explicitly at each `.map_err()` site instead.
pub fn into_acp_error(error: Error) -> acp::Error {
    match error {
        Error::Protocol(error) => error,
        Error::Application(error) => {
            acp::Error::into_internal_error(error.as_ref() as &dyn std::error::Error)
        }
        Error::Io(error) => acp::Error::into_internal_error(&error),
    }
}
