#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use anyhow::Result;
    use futures::StreamExt;
    use crate::{API, ForgeAPI};
    use crate::executor::ForgeExecutorService;
    use crate::loader::ForgeLoaderService;
    use crate::suggestion::ForgeSuggestionService;
    use forge_domain::{ConversationId, Event, Workflow, Conversation, ChatRequest, AgentMessage, ChatResponse};
    
    #[cfg(test)]
    mod retry_tests {
        use super::*;
        
        // Mock App and Infrastructure traits for testing
        use mockall::predicate::*;
        use mockall::*;
        
        mock! {
            App {
                fn conversation_service(&self) -> &dyn forge_domain::ConversationService;
                fn provider_service(&self) -> &dyn forge_domain::ProviderService;
                fn template_service(&self) -> &dyn forge_domain::TemplateService;
                fn tool_service(&self) -> &dyn forge_domain::ToolService;
                fn environment_service(&self) -> &dyn forge_domain::EnvironmentService;
                fn attachment_service(&self) -> &dyn forge_domain::AttachmentService;
            }
            
            impl Clone for App {
                fn clone(&self) -> Self;
            }
        }
        
        // Implement the Infrastructure and App traits for MockApp
        impl forge_domain::Infrastructure for MockApp {}
        impl forge_domain::App for MockApp {}

        // Test that retry returns an error when the conversation is not found
        #[tokio::test]
        async fn test_retry_conversation_not_found() -> Result<()> {
            // Arrange
            let mut mock_app = MockApp::new();
            
            // Mock conversation_service to return None
            let mut mock_conv_service = forge_domain::MockConversationService::new();
            mock_conv_service
                .expect_get()
                .returning(|_| Ok(None));
                
            mock_app
                .expect_conversation_service()
                .returning(move || &mock_conv_service);
                
            // Clone expectations for the mock
            mock_app.expect_clone().returning(move || {
                let mut clone = MockApp::new();
                clone.expect_conversation_service().returning(move || &mock_conv_service);
                clone
            });
                
            let api = ForgeAPI::new(Arc::new(mock_app));
            let conversation_id = ConversationId::generate();
            
            // Act
            let mut stream = api.retry(conversation_id)?;
            let result = stream.next().await;
            
            // Assert
            assert!(result.is_some());
            let msg = result.unwrap();
            assert!(msg.is_err());
            assert!(msg.unwrap_err().to_string().contains("Conversation not found"));
            
            Ok(())
        }
        
        // Test that retry returns an error when no message is found
        #[tokio::test]
        async fn test_retry_no_message_found() -> Result<()> {
            // Arrange
            let mut mock_app = MockApp::new();
            
            // Create test conversation with no message events
            let conversation_id = ConversationId::generate();
            let test_conversation = Conversation::new(
                conversation_id.clone(),
                Workflow::default(),
            );
            
            // Setup mock conversation service
            let mut mock_conv_service = forge_domain::MockConversationService::new();
            mock_conv_service
                .expect_get()
                .returning(move |_| Ok(Some(test_conversation.clone())));
                
            mock_app
                .expect_conversation_service()
                .returning(move || &mock_conv_service);

            // Clone expectations for the mock
            mock_app.expect_clone().returning(move || {
                let mut clone = MockApp::new();
                clone.expect_conversation_service().returning(move || &mock_conv_service);
                clone
            });
                
            let api = ForgeAPI::new(Arc::new(mock_app));
            
            // Act
            let mut stream = api.retry(conversation_id)?;
            let result = stream.next().await;
            
            // Assert
            assert!(result.is_some());
            let msg = result.unwrap();
            assert!(msg.is_err());
            assert!(msg.unwrap_err().to_string().contains("No message found to retry"));
            
            Ok(())
        }
    }
}