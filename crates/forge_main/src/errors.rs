use thiserror::Error;

#[non_exhaustive]
#[derive(Error, Debug)]
pub enum MainError {
    #[error("message")]
    AuthenticationError,
}

pub struct AuthenticationError {
    message: String,
    reason: Option<String>,
    solution: Option<String>,
}
