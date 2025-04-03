use std::collections::HashMap;

use derive_setters::Setters;
use merge::Merge;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Agent, AgentId};

#[derive(Default, Debug, Clone, Serialize, Deserialize, Merge, Setters)]
#[setters(strip_option, into)]
pub struct RetryConfig {
    /// Initial backoff delay in milliseconds for retry operations
    #[merge(strategy = crate::merge::option)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_backoff_ms: Option<u64>,

    /// Backoff multiplication factor for each retry attempt
    #[merge(strategy = crate::merge::option)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backoff_factor: Option<u64>,

    /// Maximum number of retry attempts
    #[merge(strategy = crate::merge::option)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retry_attempts: Option<usize>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, Merge, Setters)]
#[setters(strip_option)]
pub struct Workflow {
    #[merge(strategy = crate::merge::vec::unify_by_key)]
    pub agents: Vec<Agent>,

    #[merge(strategy = crate::merge::option)]
    pub variables: Option<HashMap<String, Value>>,

    #[merge(strategy = crate::merge::vec::append)]
    #[serde(default)]
    pub commands: Vec<Command>,

    /// Configuration for the retry mechanism
    #[merge(strategy = crate::merge::option)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryConfig>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, Merge, Setters)]
#[setters(strip_option, into)]
pub struct Command {
    #[merge(strategy = crate::merge::std::overwrite)]
    pub name: String,

    #[merge(strategy = crate::merge::std::overwrite)]
    pub description: String,

    #[merge(strategy = crate::merge::option)]
    pub value: Option<String>,
}

impl Workflow {
    fn find_agent(&self, id: &AgentId) -> Option<&Agent> {
        self.agents.iter().find(|a| a.id == *id)
    }

    pub fn get_agent(&self, id: &AgentId) -> crate::Result<&Agent> {
        self.find_agent(id)
            .ok_or_else(|| crate::Error::AgentUndefined(id.clone()))
    }
}
