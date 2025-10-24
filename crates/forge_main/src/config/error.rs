use thiserror::Error;

/// Config command errors
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Agent '{agent}' not found. Available agents: {available}")]
    AgentNotFound { agent: String, available: String },

    #[error("Model '{model}' not found. Available models: {available}")]
    ModelNotFound { model: String, available: String },

    #[error(
        "Provider '{provider}' is not available. Make sure the API key is set. Available providers: {available}"
    )]
    ProviderNotAvailable { provider: String, available: String },

    #[error("API error: {0}")]
    Api(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, ConfigError>;
