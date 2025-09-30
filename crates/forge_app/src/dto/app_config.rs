use derive_more::From;
use forge_domain::{AgentId, ModelId};
use serde::{Deserialize, Serialize};

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
    /// Validates that the configured model and agent are compatible
    pub fn validate(&self) -> Result<(), String> {
        // For now, we'll do basic validation - specific model-agent compatibility
        // will be handled in the service layer with access to provider information
        if let Some(model) = &self.operating_model
            && model.as_str().is_empty()
        {
            return Err("Model cannot be empty".to_string());
        }

        if let Some(agent) = &self.operating_agent
            && agent.as_str().is_empty()
        {
            return Err("Operating agent cannot be empty".to_string());
        }

        Ok(())
    }

    /// Updates the default model configuration
    pub fn set_default_model(&mut self, model: Option<ModelId>) {
        self.operating_model = model;
    }

    /// Updates the operating agent configuration
    pub fn set_operating_agent(&mut self, agent: Option<AgentId>) {
        self.operating_agent = agent;
    }
}
