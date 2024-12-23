use std::env::VarError;

use tailcall_valid::{Valid, Validator};

use crate::log::LogLevel;
use crate::Error;

#[derive(Clone, Debug)]
pub struct Cli {
    /// API Key to be used
    pub key: String,

    /// Model to be used
    pub model: Option<String>,

    /// Base URL to be used
    pub base_url: Option<String>,

    /// Log level to use
    pub log_level: Option<LogLevel>,
}

impl Cli {
    pub fn new() -> Valid<Cli, Error, Error> {
        Valid::from_option(
            std::env::var("API_KEY").ok(),
            Error::Env(VarError::NotPresent),
        )
        .zip(Valid::from_option(
            std::env::var("MODEL").ok(),
            Error::Env(VarError::NotPresent),
        ))
        .zip(Valid::from_option(
            std::env::var("BASE_URL").ok(),
            Error::Env(VarError::NotPresent),
        ))
        .zip(
            Valid::from_option(
                std::env::var("LOG_LEVEL").ok(),
                Error::Env(VarError::NotPresent),
            )
            .and_then(|v| {
                Valid::from_option(
                    LogLevel::from_str(&v),
                    Error::Custom("Invalid log level".to_string()),
                )
            }),
        )
        .map(|(((key, model), base_url), log_level)| Cli {
            key,
            model: Some(model),
            base_url: Some(base_url),
            log_level: Some(log_level),
        })
    }
}
