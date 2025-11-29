use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// Client information sent during initialization
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../vscode-extension/src/generated/")]
pub struct ClientInfo {
    pub name: String,
    pub title: String,
    pub version: String,
}

/// Server capabilities returned during initialization
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../vscode-extension/src/generated/")]
pub struct ServerCapabilities {
    pub user_agent: String,
}

/// Thread ID (maps to Conversation ID) - represented as string in TypeScript
pub type ThreadId = Uuid;

/// Turn ID (unique identifier for a single exchange) - represented as string in
/// TypeScript
pub type TurnId = Uuid;

/// Item ID (unique identifier for work units within a turn) - represented as
/// string in TypeScript
pub type ItemId = Uuid;

/// Thread/Turn/Item hierarchy types
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export, export_to = "../../../vscode-extension/src/generated/")]
pub enum ItemType {
    UserMessage,
    AgentMessage,
    ToolCall {
        tool_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(type = "Record<string, any> | undefined")]
        arguments: Option<serde_json::Value>,
    },
    FileChange {
        file_path: String,
    },
    CommandExecution {
        command: String,
    },
}

/// Item status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../vscode-extension/src/generated/")]
pub enum ItemStatus {
    Started,
    Completed,
    Failed,
}

/// Turn status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../vscode-extension/src/generated/")]
pub enum TurnStatus {
    Started,
    Completed,
    Failed,
    Cancelled,
}

/// File change details for approval
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../vscode-extension/src/generated/")]
pub struct FileChangeDetails {
    pub file_path: String,
    pub original_content: Option<String>,
    pub new_content: String,
    pub diff: Option<String>,
}

/// Command execution details for approval
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../vscode-extension/src/generated/")]
pub struct CommandExecutionDetails {
    pub command: String,
    pub working_directory: String,
}

/// Approval decision
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../vscode-extension/src/generated/")]
pub enum ApprovalDecision {
    Accept,
    Reject,
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    // This test triggers ts-rs type generation
    #[test]
    fn export_typescript_types() {
        // Export all types
        ClientInfo::export().expect("Failed to export ClientInfo");
        ServerCapabilities::export().expect("Failed to export ServerCapabilities");
        ItemType::export().expect("Failed to export ItemType");
        ItemStatus::export().expect("Failed to export ItemStatus");
        TurnStatus::export().expect("Failed to export TurnStatus");
        FileChangeDetails::export().expect("Failed to export FileChangeDetails");
        CommandExecutionDetails::export().expect("Failed to export CommandExecutionDetails");
        ApprovalDecision::export().expect("Failed to export ApprovalDecision");

        println!("✅ TypeScript types exported successfully");
    }

    #[test]
    fn test_client_info_serialization() {
        let info = ClientInfo {
            name: "forge-vscode".to_string(),
            title: "Forge VSCode Extension".to_string(),
            version: "0.1.0".to_string(),
        };

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: ClientInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, info.name);
        assert_eq!(deserialized.version, info.version);
    }

    #[test]
    fn test_item_status_serialization() {
        let status = ItemStatus::Started;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"started\"");

        let deserialized: ItemStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ItemStatus::Started);
    }

    #[test]
    fn test_approval_decision() {
        let decision = ApprovalDecision::Accept;
        let json = serde_json::to_string(&decision).unwrap();
        assert_eq!(json, "\"accept\"");
    }
}
