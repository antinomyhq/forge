use serde::{Deserialize, Serialize};

use super::{ApprovalDecision, ClientInfo, ItemId, ThreadId, TurnId};

/// JSON-RPC 2.0 request structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<i64>,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

/// Client requests sent to the server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "camelCase")]
pub enum ClientRequest {
    /// Initialize the server
    #[serde(rename = "initialize")]
    Initialize { client_info: ClientInfo },

    /// Notification that initialization is complete
    #[serde(rename = "initialized")]
    Initialized,

    /// Start a new thread (conversation)
    #[serde(rename = "thread/start")]
    ThreadStart {
        thread_id: Option<ThreadId>,
        agent_id: Option<String>,
    },

    /// List all threads
    #[serde(rename = "thread/list")]
    ThreadList { limit: Option<usize> },

    /// Get thread details
    #[serde(rename = "thread/get")]
    ThreadGet { thread_id: ThreadId },

    /// Start a new turn in a thread
    #[serde(rename = "turn/start")]
    TurnStart {
        thread_id: ThreadId,
        turn_id: TurnId,
        message: String,
        files: Option<Vec<String>>,
    },

    /// Retry the last turn
    #[serde(rename = "turn/retry")]
    TurnRetry { thread_id: ThreadId },

    /// Cancel an ongoing turn
    #[serde(rename = "turn/cancel")]
    TurnCancel {
        thread_id: ThreadId,
        turn_id: TurnId,
    },

    /// Compact a thread
    #[serde(rename = "thread/compact")]
    ThreadCompact { thread_id: ThreadId },

    /// Set active agent
    #[serde(rename = "agent/set")]
    AgentSet { agent_id: String },

    /// List available agents
    #[serde(rename = "agent/list")]
    AgentList,

    /// List available models
    #[serde(rename = "model/list")]
    ModelList,

    /// Set default model
    #[serde(rename = "model/set")]
    ModelSet { model_id: String },

    /// List providers
    #[serde(rename = "provider/list")]
    ProviderList,

    /// Set default provider
    #[serde(rename = "provider/set")]
    ProviderSet { provider_id: String },

    /// Generate commit message
    #[serde(rename = "git/commit")]
    GitCommit {
        preview: bool,
        max_diff_size: Option<usize>,
        additional_context: Option<String>,
    },

    /// Generate shell command from natural language
    #[serde(rename = "command/suggest")]
    CommandSuggest { prompt: String },

    /// List available skills
    #[serde(rename = "skill/list")]
    SkillList,

    /// List custom commands
    #[serde(rename = "command/list")]
    CommandList,

    /// Get environment info
    #[serde(rename = "env/info")]
    EnvInfo,

    /// File change approval response
    #[serde(rename = "approval/fileChange")]
    ApprovalFileChange {
        item_id: ItemId,
        decision: ApprovalDecision,
    },

    /// Command execution approval response
    #[serde(rename = "approval/commandExecution")]
    ApprovalCommandExecution {
        item_id: ItemId,
        decision: ApprovalDecision,
    },
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_json_rpc_request_parsing() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"test","title":"Test","version":"1.0.0"}}}"#;
        let request: JsonRpcRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.jsonrpc, "2.0");
        assert_eq!(request.id, Some(1));
        assert_eq!(request.method, "initialize");
    }

    #[test]
    fn test_thread_start_request() {
        let request =
            ClientRequest::ThreadStart { thread_id: None, agent_id: Some("forge".to_string()) };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["method"], "thread/start");
    }

    #[test]
    fn test_initialized_notification() {
        let request = ClientRequest::Initialized;
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["method"], "initialized");
    }

    #[test]
    fn test_env_info_request() {
        let json = serde_json::json!({
            "method": "env/info"
        });
        let request: ClientRequest = serde_json::from_value(json).unwrap();

        match request {
            ClientRequest::EnvInfo => {}
            _ => panic!("Expected EnvInfo variant"),
        }
    }

    #[test]
    fn test_env_info_request_with_empty_params() {
        let json = serde_json::json!({
            "method": "env/info",
            "params": {}
        });
        let request: ClientRequest = serde_json::from_value(json).unwrap();

        match request {
            ClientRequest::EnvInfo => {}
            _ => panic!("Expected EnvInfo variant"),
        }
    }
}
