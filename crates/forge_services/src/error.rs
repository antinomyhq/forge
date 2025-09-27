use forge_app::dto::ProviderId;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("Environment variable {env_var} not found for provider {provider}")]
    EnvironmentVariableNotFound {
        provider: ProviderId,
        env_var: String,
    },

    #[error("Provider {provider} is not available via environment configuration")]
    ProviderNotAvailable { provider: ProviderId },

    #[error("Failed to create VertexAI provider: {message}")]
    VertexAiConfiguration { message: String },
}

impl ProviderError {
    pub fn env_var_not_found(provider: ProviderId, env_var: &str) -> Self {
        Self::EnvironmentVariableNotFound { provider, env_var: env_var.to_string() }
    }

    pub fn provider_not_available(provider: ProviderId) -> Self {
        Self::ProviderNotAvailable { provider }
    }

    pub fn vertex_ai_config(message: impl Into<String>) -> Self {
        Self::VertexAiConfiguration { message: message.into() }
    }
}
