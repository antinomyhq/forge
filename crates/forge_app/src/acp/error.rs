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

impl From<Error> for acp::Error {
    fn from(error: Error) -> Self {
        match error {
            Error::Protocol(error) => error,
            Error::Application(error) => {
                acp::Error::into_internal_error(error.as_ref() as &dyn std::error::Error)
            }
            Error::Io(error) => acp::Error::into_internal_error(&error),
        }
    }
}