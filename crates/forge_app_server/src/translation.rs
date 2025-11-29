use forge_api::ChatResponse;
use uuid::Uuid;

use crate::protocol::{ItemId, ItemStatus, ItemType, ServerNotification, ThreadId, TurnId};

/// Translates ForgeAPI ChatResponse events into protocol notifications
pub struct EventTranslator {
    thread_id: ThreadId,
    turn_id: TurnId,
    current_item_id: Option<ItemId>,
}

impl EventTranslator {
    /// Creates a new EventTranslator for a specific thread and turn
    pub fn new(thread_id: ThreadId, turn_id: TurnId) -> Self {
        Self { thread_id, turn_id, current_item_id: None }
    }

    /// Forwards ChatResponse as-is (new approach - matches terminal UI pattern)
    pub fn forward(&self, response: ChatResponse) -> ServerNotification {
        ServerNotification::ChatEvent {
            thread_id: self.thread_id,
            turn_id: self.turn_id,
            event: response,
        }
    }

    /// Translates a ChatResponse into zero or more ServerNotifications
    /// (DEPRECATED) Use `forward()` instead for direct event forwarding
    pub fn translate(&mut self, response: ChatResponse) -> Vec<ServerNotification> {
        match response {
            ChatResponse::TaskMessage { content } => self.translate_task_message(content),
            ChatResponse::TaskReasoning { content } => self.translate_reasoning(content),
            ChatResponse::TaskComplete => self.translate_task_complete(),
            ChatResponse::ToolCallStart(tool_call) => self.translate_tool_call_start(tool_call),
            ChatResponse::ToolCallEnd(tool_result) => self.translate_tool_call_end(tool_result),
            ChatResponse::Usage(usage) => self.translate_usage(usage),
            ChatResponse::RetryAttempt { cause, duration } => self.translate_retry(cause, duration),
            ChatResponse::Interrupt { reason } => self.translate_interrupt(reason),
        }
    }

    fn translate_task_message(
        &mut self,
        content: forge_api::ChatResponseContent,
    ) -> Vec<ServerNotification> {
        let mut notifications = Vec::new();

        // Start agent message item if not already started
        if self.current_item_id.is_none() {
            let item_id = Uuid::new_v4();
            self.current_item_id = Some(item_id);

            notifications.push(ServerNotification::ItemStarted {
                thread_id: self.thread_id,
                turn_id: self.turn_id,
                item_id,
                item_type: ItemType::AgentMessage,
            });
        }

        // Extract text content
        let text = match content {
            forge_api::ChatResponseContent::PlainText(t) => t,
            forge_api::ChatResponseContent::Markdown(m) => m,
            forge_api::ChatResponseContent::Title(title) => title.title,
        };

        // Send delta notification
        if let Some(item_id) = self.current_item_id {
            notifications.push(ServerNotification::AgentMessageDelta {
                thread_id: self.thread_id,
                turn_id: self.turn_id,
                item_id,
                delta: text,
            });
        }

        notifications
    }

    fn translate_reasoning(&mut self, content: String) -> Vec<ServerNotification> {
        let mut notifications = Vec::new();

        // Start reasoning item if not already started
        if self.current_item_id.is_none() {
            let item_id = Uuid::new_v4();
            self.current_item_id = Some(item_id);

            notifications.push(ServerNotification::ItemStarted {
                thread_id: self.thread_id,
                turn_id: self.turn_id,
                item_id,
                item_type: ItemType::AgentMessage,
            });
        }

        // Send reasoning delta
        if let Some(item_id) = self.current_item_id {
            notifications.push(ServerNotification::AgentReasoningDelta {
                thread_id: self.thread_id,
                turn_id: self.turn_id,
                item_id,
                delta: content,
            });
        }

        notifications
    }

    fn translate_task_complete(&mut self) -> Vec<ServerNotification> {
        let mut notifications = Vec::new();

        // Complete current item if any
        if let Some(item_id) = self.current_item_id.take() {
            notifications.push(ServerNotification::ItemCompleted {
                thread_id: self.thread_id,
                turn_id: self.turn_id,
                item_id,
                status: ItemStatus::Completed,
            });
        }

        // Complete the turn
        notifications.push(ServerNotification::TurnCompleted {
            thread_id: self.thread_id,
            turn_id: self.turn_id,
            status: crate::protocol::TurnStatus::Completed,
        });

        notifications
    }

    fn translate_tool_call_start(
        &mut self,
        tool_call: forge_api::ToolCallFull,
    ) -> Vec<ServerNotification> {
        let mut notifications = Vec::new();

        // Complete previous item if any
        if let Some(item_id) = self.current_item_id.take() {
            notifications.push(ServerNotification::ItemCompleted {
                thread_id: self.thread_id,
                turn_id: self.turn_id,
                item_id,
                status: ItemStatus::Completed,
            });
        }

        // Start tool call item
        let item_id = Uuid::new_v4();
        self.current_item_id = Some(item_id);

        // Serialize tool call arguments to JSON
        let arguments = serde_json::to_value(&tool_call.arguments).ok();

        notifications.push(ServerNotification::ItemStarted {
            thread_id: self.thread_id,
            turn_id: self.turn_id,
            item_id,
            item_type: ItemType::ToolCall { tool_name: tool_call.name.to_string(), arguments },
        });

        notifications
    }

    fn translate_tool_call_end(
        &mut self,
        _tool_result: forge_api::ToolResult,
    ) -> Vec<ServerNotification> {
        let mut notifications = Vec::new();

        // Complete tool call item
        if let Some(item_id) = self.current_item_id.take() {
            notifications.push(ServerNotification::ItemCompleted {
                thread_id: self.thread_id,
                turn_id: self.turn_id,
                item_id,
                status: ItemStatus::Completed,
            });
        }

        notifications
    }

    fn translate_usage(&self, usage: forge_api::Usage) -> Vec<ServerNotification> {
        vec![ServerNotification::TurnUsage {
            thread_id: self.thread_id,
            turn_id: self.turn_id,
            input_tokens: *usage.prompt_tokens as u64,
            output_tokens: *usage.completion_tokens as u64,
            total_cost: usage.cost,
        }]
    }

    fn translate_retry(
        &self,
        cause: forge_api::Cause,
        _duration: std::time::Duration,
    ) -> Vec<ServerNotification> {
        vec![ServerNotification::Progress {
            message: format!("Retrying due to: {:?}", cause),
            percentage: None,
        }]
    }

    fn translate_interrupt(
        &self,
        reason: forge_api::InterruptionReason,
    ) -> Vec<ServerNotification> {
        vec![ServerNotification::Progress {
            message: format!("Interrupted: {:?}", reason),
            percentage: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use forge_api::ChatResponseContent;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_translate_task_message() {
        let thread_id = Uuid::new_v4();
        let turn_id = Uuid::new_v4();
        let mut translator = EventTranslator::new(thread_id, turn_id);

        let response = ChatResponse::TaskMessage {
            content: ChatResponseContent::PlainText("Hello".to_string()),
        };

        let notifications = translator.translate(response);
        assert_eq!(notifications.len(), 2); // ItemStarted + AgentMessageDelta

        match &notifications[0] {
            ServerNotification::ItemStarted { item_type, .. } => {
                assert!(matches!(item_type, ItemType::AgentMessage));
            }
            _ => panic!("Expected ItemStarted"),
        }

        match &notifications[1] {
            ServerNotification::AgentMessageDelta { delta, .. } => {
                assert_eq!(delta, "Hello");
            }
            _ => panic!("Expected AgentMessageDelta"),
        }
    }

    #[test]
    fn test_translate_task_complete() {
        let thread_id = Uuid::new_v4();
        let turn_id = Uuid::new_v4();
        let mut translator = EventTranslator::new(thread_id, turn_id);

        // Start an item first
        translator.current_item_id = Some(Uuid::new_v4());

        let response = ChatResponse::TaskComplete;
        let notifications = translator.translate(response);

        assert_eq!(notifications.len(), 2); // ItemCompleted + TurnCompleted
    }
}
