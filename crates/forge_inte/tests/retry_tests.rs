#[cfg(test)]
mod retry_integration_tests {
    use std::sync::Arc;
    use anyhow::Result;
    use forge_api::{API, ChatRequest, ConversationId, Event, ForgeAPI};
    use forge_domain::{AgentMessage, Workflow, ChatResponse};
    use futures::StreamExt;

    #[tokio::test]
    async fn test_retry_functionality() -> Result<()> {
        // Initialize the API with default (restricted) mode
        let api = Arc::new(ForgeAPI::init(true));
        
        // Create a test workflow and conversation
        let workflow = api.load(None).await?;
        let conversation_id = api.init(workflow).await?;
        
        // Send a test message
        let test_message = "Test message for retry";
        let event = Event::new("message", test_message);
        let chat_request = ChatRequest::new(event, conversation_id.clone());
        
        // Process the original message
        let mut stream = api.chat(chat_request).await?;
        // Just drain the stream - we don't need to check the responses for this test
        while let Some(_) = stream.next().await {}
        
        // Now retry the message
        let mut retry_stream = api.retry(conversation_id).await?;
        
        // Collect response from retry
        let mut responses = Vec::new();
        while let Some(res) = retry_stream.next().await {
            if let Ok(msg) = res {
                if let ChatResponse::Text(text) = msg.message {
                    responses.push(text);
                }
            }
        }
        
        // There should be some responses (we don't care about the exact content)
        assert!(!responses.is_empty());
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_retry_unknown_conversation() -> Result<()> {
        // Initialize the API
        let api = Arc::new(ForgeAPI::init(true));
        
        // Try to retry a non-existent conversation
        let unknown_id = ConversationId::generate();
        let mut stream = api.retry(unknown_id)?;
        
        // The first message should be an error about the conversation not found
        let first_message = stream.next().await;
        assert!(first_message.is_some());
        
        let result = first_message.unwrap();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Conversation not found"));
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_retry_no_messages() -> Result<()> {
        // Initialize the API
        let api = Arc::new(ForgeAPI::init(true));
        
        // Create a conversation but don't send any messages
        let workflow = api.load(None).await?;
        let conversation_id = api.init(workflow).await?;
        
        // Try to retry when there are no messages
        let mut stream = api.retry(conversation_id)?;
        
        // The first message should be an error about no message to retry
        let first_message = stream.next().await;
        assert!(first_message.is_some());
        
        let result = first_message.unwrap();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No message found to retry"));
        
        Ok(())
    }
}