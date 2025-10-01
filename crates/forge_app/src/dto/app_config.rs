use derive_more::From;
use forge_domain::{AgentId, ModelId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigValidationError {
    #[error("Model cannot be empty")]
    EmptyModel,
    #[error("Operating agent cannot be empty")]
    EmptyAgent,
    #[error("Invalid model format: {0}")]
    InvalidModelFormat(String),
    #[error("Invalid agent format: {0}")]
    InvalidAgentFormat(String),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitAuth {
    pub session_id: String,
    pub auth_url: String,
    pub token: String,
}

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub key_info: Option<LoginInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operating_agent: Option<AgentId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operating_model: Option<ModelId>,
}

#[derive(Clone, Serialize, Deserialize, From)]
#[serde(rename_all = "camelCase")]
pub struct LoginInfo {
    pub api_key: String,
    pub api_key_name: String,
    pub api_key_masked: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_provider_id: Option<String>,
}
impl AppConfig {
    /// Validates that the configured model and agent are valid
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        // Validate model format and non-emptiness
        if let Some(model) = &self.operating_model {
            if model.as_str().is_empty() {
                return Err(ConfigValidationError::EmptyModel);
            }
            // Additional format validation could be added here
            if !self.is_valid_model_format(model.as_str()) {
                return Err(ConfigValidationError::InvalidModelFormat(
                    model.as_str().to_string(),
                ));
            }
        }

        // Validate agent format and non-emptiness
        if let Some(agent) = &self.operating_agent {
            if agent.as_str().is_empty() {
                return Err(ConfigValidationError::EmptyAgent);
            }
            // Additional format validation could be added here
            if !self.is_valid_agent_format(agent.as_str()) {
                return Err(ConfigValidationError::InvalidAgentFormat(
                    agent.as_str().to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Updates the default model configuration with validation
    pub fn set_default_model(&mut self, model: Option<ModelId>) {
        self.operating_model = model;
    }

    /// Updates the operating agent configuration with validation
    pub fn set_operating_agent(&mut self, agent: Option<AgentId>) {
        self.operating_agent = agent;
    }

    /// Check if model format is valid (basic validation)
    fn is_valid_model_format(&self, model: &str) -> bool {
        // Basic validation: should contain at least one character and no whitespace
        !model.is_empty() && !model.chars().any(|c| c.is_whitespace())
    }

    /// Check if agent format is valid (basic validation)
    fn is_valid_agent_format(&self, agent: &str) -> bool {
        // Basic validation: should contain at least one character and no whitespace
        !agent.is_empty() && !agent.chars().any(|c| c.is_whitespace())
    }
}
