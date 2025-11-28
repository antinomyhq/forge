use serde::{Deserialize, Serialize};

use super::{
    CommandExecutionDetails, FileChangeDetails, ItemId, ItemStatus, ItemType, ServerCapabilities,
    ThreadId, TurnId, TurnStatus,
};

/// JSON-RPC 2.0 response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 error structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 notification structure (no id field)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: serde_json::Value,
}

/// Server notifications sent to the client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "camelCase")]
pub enum ServerNotification {
    /// Thread has started
    #[serde(rename = "thread/started")]
    ThreadStarted { thread_id: ThreadId },

    /// Turn has started
    #[serde(rename = "turn/started")]
    TurnStarted {
        thread_id: ThreadId,
        turn_id: TurnId,
    },

    /// Turn has completed
    #[serde(rename = "turn/completed")]
    TurnCompleted {
        thread_id: ThreadId,
        turn_id: TurnId,
        status: TurnStatus,
    },

    /// Item has started
    #[serde(rename = "item/started")]
    ItemStarted {
        thread_id: ThreadId,
        turn_id: TurnId,
        item_id: ItemId,
        item_type: ItemType,
    },

    /// Item has completed
    #[serde(rename = "item/completed")]
    ItemCompleted {
        thread_id: ThreadId,
        turn_id: TurnId,
        item_id: ItemId,
        status: ItemStatus,
    },

    /// Agent message delta (streaming)
    #[serde(rename = "item/agentMessage/delta")]
    AgentMessageDelta {
        thread_id: ThreadId,
        turn_id: TurnId,
        item_id: ItemId,
        delta: String,
    },

    /// Agent reasoning (thinking process)
    #[serde(rename = "item/agentReasoning/delta")]
    AgentReasoningDelta {
        thread_id: ThreadId,
        turn_id: TurnId,
        item_id: ItemId,
        delta: String,
    },

    /// Command execution output delta
    #[serde(rename = "item/commandExecution/output")]
    CommandExecutionOutput {
        thread_id: ThreadId,
        turn_id: TurnId,
        item_id: ItemId,
        output: String,
    },

    /// Token usage update
    #[serde(rename = "turn/usage")]
    TurnUsage {
        thread_id: ThreadId,
        turn_id: TurnId,
        input_tokens: u64,
        output_tokens: u64,
        total_cost: Option<f64>,
    },

    /// Progress update
    #[serde(rename = "progress")]
    Progress {
        message: String,
        percentage: Option<f32>,
    },
}

/// Server requests sent to the client (requiring a response)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "camelCase")]
pub enum ServerRequest {
    /// Request approval for file change
    #[serde(rename = "item/fileChange/requestApproval")]
    FileChangeRequestApproval {
        thread_id: ThreadId,
        turn_id: TurnId,
        item_id: ItemId,
        details: FileChangeDetails,
    },

    /// Request approval for command execution
    #[serde(rename = "item/commandExecution/requestApproval")]
    CommandExecutionRequestApproval {
        thread_id: ThreadId,
        turn_id: TurnId,
        item_id: ItemId,
        details: CommandExecutionDetails,
    },
}

/// Initialize response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResponse {
    pub capabilities: ServerCapabilities,
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use uuid::Uuid;

    use super::*;

    #[test]
    fn test_json_rpc_response() {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: 1,
            result: Some(serde_json::json!({"success": true})),
            error: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"id\":1"));
    }

    #[test]
    fn test_json_rpc_error() {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: 1,
            result: None,
            error: Some(JsonRpcError {
                code: -32600,
                message: "Invalid Request".to_string(),
                data: None,
            }),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""));
        assert!(json.contains("-32600"));
    }

    #[test]
    fn test_server_notification() {
        let notification = ServerNotification::ThreadStarted { thread_id: Uuid::new_v4() };

        let json = serde_json::to_value(&notification).unwrap();
        assert_eq!(json["method"], "thread/started");
    }

    #[test]
    fn test_agent_message_delta() {
        let notification = ServerNotification::AgentMessageDelta {
            thread_id: Uuid::new_v4(),
            turn_id: Uuid::new_v4(),
            item_id: Uuid::new_v4(),
            delta: "Hello".to_string(),
        };

        let json = serde_json::to_value(&notification).unwrap();
        assert_eq!(json["method"], "item/agentMessage/delta");
        assert_eq!(json["params"]["delta"], "Hello");
    }

    #[test]
    fn test_item_started_serialization() {
        let notification = ServerNotification::ItemStarted {
            thread_id: Uuid::new_v4(),
            turn_id: Uuid::new_v4(),
            item_id: Uuid::new_v4(),
            item_type: crate::protocol::ItemType::AgentMessage,
        };

        let json = serde_json::to_value(&notification).unwrap();
        assert_eq!(json["method"], "item/started");

        // Print to see actual structure
        println!(
            "ItemStarted JSON: {}",
            serde_json::to_string_pretty(&json).unwrap()
        );

        // ItemType::AgentMessage should serialize as string "AgentMessage"
        assert_eq!(json["params"]["itemType"], "AgentMessage");
    }
}
