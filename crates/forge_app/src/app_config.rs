use std::collections::HashMap;

use derive_more::{Display, From};
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_info: Option<LoginInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choices: Option<ChoicesConfig>,
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

#[derive(Default, Debug, Clone, Serialize, Deserialize, Display, PartialEq, EnumIter)]
#[serde(rename_all = "camelCase")]
pub enum ChoiceType {
    Allow,
    #[default]
    #[display("Ask every time")]
    AskEveryTime,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoicesConfig {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub execute_shell_commands: HashMap<String, ChoiceType>,
}
#[derive(Clone, Debug, Display, EnumIter)]
pub enum ConfigOption {
    #[display("Shell Command Execution")]
    ShellCommandExecution,
}
