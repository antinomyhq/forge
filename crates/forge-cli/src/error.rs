use std::fmt::{Debug, Display, Formatter};

use derive_more::derive::{Display, From};
use derive_setters::Setters;

#[derive(Display, From)]
pub enum Error {
    Inquire(inquire::InquireError),
    // TODO: drop `Custom` because its too generic
    Custom(String),
    Provider(forge_provider::Error),
    IO(std::io::Error),
    SendError(tokio::sync::broadcast::error::SendError<std::string::String>),
    Serde(serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&Errata::from(self), f)
    }
}

#[derive(Clone, Setters)]
pub struct Errata {
    pub title: String,
    #[setters(strip_option, into)]
    pub description: Option<String>,
}

impl Errata {
    pub fn new(title: impl Into<String>) -> Self {
        Self { title: title.into(), description: None }
    }
}

impl Display for Errata {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.title)
    }
}

impl Debug for Errata {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.title)?;
        if let Some(desc) = &self.description {
            if !desc.trim().is_empty() {
                write!(f, "\n{}", desc)?;
            }
        }
        Ok(())
    }
}

impl From<&Error> for Errata {
    fn from(error: &Error) -> Self {
        match error {
            Error::Inquire(error) => Errata::new(format!("{}", error)),
            Error::Custom(error) => Errata::new(error.to_string()),
            Error::Provider(error) => Errata::new(format!("{}", error)),
            Error::IO(error) => Errata::new(format!("{}", error)),
            Error::SendError(send_error) => Errata::new(format!("{}", send_error)),
            Error::Serde(error) => Errata::new(format!("{}", error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_simple_error() {
        let error = Errata::new("Something went wrong");
        assert_eq!(format!("{:?}", error), indoc! {"Something went wrong"});
    }

    #[test]
    fn test_error_with_description() {
        let error = Errata::new("Invalid input").description("Expected a number");
        assert_eq!(
            format!("{:?}", error),
            indoc! {"
                Invalid input
                Expected a number"}
        );
    }
}
