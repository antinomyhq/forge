use derive_more::derive::From;

#[derive(Debug, From)]
pub enum Error {
    IO(std::io::Error),
    Handlebars(handlebars::RenderError),
    IndeterminateShell(Platform),
    IndeterminateHomeDir,
}

#[derive(Debug)]
pub enum Platform {
    Windows,
    UnixLike,
}

pub type Result<T> = std::result::Result<T, Error>;
