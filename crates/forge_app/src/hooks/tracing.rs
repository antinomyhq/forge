use async_trait::async_trait;
use forge_domain::{
    Conversation, EndPayload, EventData, EventHandle, FinishReason, RequestPayload,
    ResponsePayload, ToolcallEndPayload, ToolcallStartPayload,
};
use tracing::{debug, info, warn};

/// Handler that provides comprehensive tracing/logging for all lifecycle events
///
/// This handler logs important information at various stages of the orchestration:
/// - Start: Logs conversation and agent initialization
/// - Request: Logs each request iteration
/// - Response: Logs token usage, costs, and conversation metrics
/// - ToolcallStart: Logs tool execution start
/// - ToolcallEnd: Logs tool failures with details
/// - End: Logs title generation when available
#[derive(Clone)]
pub struct TracingHandler;

impl TracingHandler {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl EventHandle<EventData<RequestPayload>> for TracingHandler {
    async fn handle(
        &self,
        event: &EventData<RequestPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let request_count = event.payload.request_count;

        // Log at first request (initialization)
        if request_count == 0 {
            debug!(
                conversation_id = %conversation.id,
                agent = %event.agent.id,
                model = %event.model_id,
                "Initializing agent"
            );
        }

        Ok(())
    }
}

#[async_trait]
impl EventHandle<EventData<ResponsePayload>> for TracingHandler {
    async fn handle(
        &self,
        event: &EventData<ResponsePayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let message = &event.payload.message;

        if let Some(context) = &conversation.context {
            info!(
                conversation_id = %conversation.id,
                conversation_length = context.messages.len(),
                token_usage = format!("{}", message.usage.prompt_tokens),
                total_tokens = format!("{}", message.usage.total_tokens),
                cached_tokens = format!("{}", message.usage.cached_tokens),
                cost = message.usage.cost.unwrap_or_default(),
                finish_reason = message.finish_reason.as_ref().map_or("", |reason| reason.into()),
                "Processing usage information"
            );
        }

        debug!(
            agent_id = %event.agent.id,
            tool_call_count = message.tool_calls.len(),
            "Tool call count"
        );

        Ok(())
    }
}

#[async_trait]
impl EventHandle<EventData<ToolcallStartPayload>> for TracingHandler {
    async fn handle(
        &self,
        event: &EventData<ToolcallStartPayload>,
        _conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let tool_call = &event.payload.tool_call;

        debug!(
            agent_id = %event.agent.id,
            tool_name = %tool_call.name,
            call_id = ?tool_call.call_id,
            "Tool call started"
        );

        Ok(())
    }
}

#[async_trait]
impl EventHandle<EventData<ToolcallEndPayload>> for TracingHandler {
    async fn handle(
        &self,
        event: &EventData<ToolcallEndPayload>,
        _conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let result = &event.payload.result;

        if result.is_error() {
            warn!(
                agent_id = %event.agent.id,
                name = %result.name,
                call_id = ?result.call_id,
                output = ?result.output,
                "Tool call failed",
            );
        }

        Ok(())
    }
}

#[async_trait]
impl EventHandle<EventData<EndPayload>> for TracingHandler {
    async fn handle(
        &self,
        _event: &EventData<EndPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        if let Some(title) = &conversation.title {
            debug!(
                conversation_id = %conversation.id,
                title,
                "Title generated for conversation"
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_domain::{Agent, ChatCompletionMessageFull, ModelId, ToolResult, ToolCallId, ToolName};

    fn test_agent() -> Agent {
        Agent::new("test-agent", "test-provider".to_string().into(), ModelId::new("test-model"))
    }

    fn test_model_id() -> ModelId {
        ModelId::new("test-model")
    }

    #[tokio::test]
    async fn test_tracing_handler_request() {
        let handler = TracingHandler::new();
        let mut conversation = Conversation::generate();
        let event = EventData::new(test_agent(), test_model_id(), RequestPayload::new(0));

        // Should not panic
        handler.handle(&event, &mut conversation).await.unwrap();
    }

    #[tokio::test]
    async fn test_tracing_handler_response() {
        let handler = TracingHandler::new();
        let mut conversation = Conversation::generate();
        let message = ChatCompletionMessageFull {
            content: "test".to_string(),
            thought_signature: None,
            reasoning: None,
            reasoning_details: None,
            tool_calls: vec![],
            usage: Default::default(),
            finish_reason: None,
        };
        let event = EventData::new(test_agent(), test_model_id(), ResponsePayload::new(message));

        // Should not panic
        handler.handle(&event, &mut conversation).await.unwrap();
    }

    #[tokio::test]
    async fn test_tracing_handler_toolcall_end_error() {
        let handler = TracingHandler::new();
        let mut conversation = Conversation::generate();
        let result = ToolResult::new(ToolName::from("test-tool"))
            .call_id(ToolCallId::new("test-id"))
            .failure(anyhow::anyhow!("Test error"));
        let event = EventData::new(test_agent(), test_model_id(), ToolcallEndPayload::new(result));

        // Should log warning but not panic
        handler.handle(&event, &mut conversation).await.unwrap();
    }

    #[tokio::test]
    async fn test_tracing_handler_end_with_title() {
        let handler = TracingHandler::new();
        let mut conversation = Conversation::generate().title(Some("Test Title".to_string()));
        let event = EventData::new(test_agent(), test_model_id(), EndPayload);

        // Should log debug message with title
        handler.handle(&event, &mut conversation).await.unwrap();
    }
}
