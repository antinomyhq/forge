use std::path::PathBuf;

use anyhow::Context;
use forge_api::{
    AgentMessage, ChatRequest, ChatResponse, Conversation, ConversationId, ForgeAPI, ModelId, API,
};
use tokio_stream::StreamExt;

const MAX_RETRIES: usize = 5;
const WORKFLOW_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/test_workflow.yaml");

/// Test fixture for API testing that supports parallel model validation
struct Fixture {
    model: ModelId,
    #[allow(dead_code)] // The guard is kept alive by being held in the struct
    _guard: forge_tracker::Guard,
}

impl Fixture {
    /// Create a new test fixture with the given task
    fn new(model: ModelId) -> Self {
        Self {
            model,
            _guard: forge_tracker::init_tracing(PathBuf::from(".")).unwrap(),
        }
    }

    /// Get the API service, panicking if not validated
    fn api(&self) -> impl API {
        // NOTE: In tests the CWD is not the project root
        ForgeAPI::init(true)
    }

    /// Get model response as text
    async fn get_model_response(&self) -> String {
        let api = self.api();
        // load the workflow from path
        let mut workflow = api.load(Some(&PathBuf::from(WORKFLOW_PATH))).await.unwrap();

        // in workflow, replace all models with the model we want to test.
        workflow.agents.iter_mut().for_each(|agent| {
            agent.model = self.model.clone();
        });

        // initialize the conversation by storing the workflow.
        let conversation_id = api.init(workflow).await.unwrap();
        let request = ChatRequest::new(
            "There is a cat hidden in the codebase. What is its name?",
            conversation_id,
        );

        api.chat(request)
            .await
            .with_context(|| "Failed to initialize chat")
            .unwrap()
            .filter_map(|message| match message.unwrap() {
                AgentMessage { message: ChatResponse::Text(text), .. } => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>()
            .await
            .join("")
            .trim()
            .to_string()
    }

    /// Test single model with retries
    async fn test_single_model(&self, check_response: impl Fn(&str) -> bool) -> Result<(), String> {
        for attempt in 0..MAX_RETRIES {
            let response = self.get_model_response().await;

            if check_response(&response) {
                println!(
                    "[{}] Successfully checked response in {} attempts",
                    self.model,
                    attempt + 1
                );
                return Ok(());
            }

            if attempt < MAX_RETRIES - 1 {
                println!("[{}] Attempt {}/{}", self.model, attempt + 1, MAX_RETRIES);
            }
        }
        Err(format!(
            "[{}] Failed after {} attempts",
            self.model, MAX_RETRIES
        ))
    }

    /// Initialize a conversation with a specific message and return the
    /// conversation ID and response
    async fn init_conversation_with_message(&self, message: &str) -> (ConversationId, String) {
        let api = self.api();
        // load the workflow from path
        let mut workflow = api.load(Some(&PathBuf::from(WORKFLOW_PATH))).await.unwrap();

        // in workflow, replace all models with the model we want to test.
        workflow.agents.iter_mut().for_each(|agent| {
            agent.model = self.model.clone();
        });

        // initialize the conversation by storing the workflow.
        let conversation_id = api.init(workflow).await.unwrap();

        // send the message
        let response = self.send_message(message, &conversation_id).await;

        (conversation_id, response)
    }

    /// Send a message to a conversation and return the response
    async fn send_message(&self, message: &str, conversation_id: &ConversationId) -> String {
        let api = self.api();
        let request = ChatRequest::new(message, conversation_id.clone());

        api.chat(request)
            .await
            .unwrap()
            .filter_map(|message| match message {
                Ok(AgentMessage { message: ChatResponse::Text(text), .. }) => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>()
            .await
            .join("")
            .trim()
            .to_string()
    }

    /// Retry a conversation and return the response
    async fn retry(&self, conversation_id: &ConversationId) -> String {
        let api = self.api();

        api.retry(conversation_id.clone())
            .await
            .unwrap()
            .filter_map(|message| match message.unwrap() {
                AgentMessage { message: ChatResponse::Text(text), .. } => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>()
            .await
            .join("")
            .trim()
            .to_string()
    }

    /// Get a conversation by ID
    async fn get_conversation(&self, conversation_id: &ConversationId) -> Option<Conversation> {
        let api = self.api();
        api.conversation(conversation_id).await.unwrap()
    }

    /// Test retry with multiple attempts
    async fn test_retry_with_attempts(
        &self,
        conversation_id: &ConversationId,
        attempts: usize,
        validate_fn: impl Fn(&str) -> bool,
    ) -> Result<(), String> {
        for attempt in 0..attempts {
            let response = self.retry(conversation_id).await;

            if validate_fn(&response) {
                println!(
                    "[{}] Successfully validated retry response in {} attempts",
                    self.model,
                    attempt + 1
                );
                return Ok(());
            }

            if attempt < attempts - 1 {
                println!(
                    "[{}] Retry attempt {}/{}",
                    self.model,
                    attempt + 1,
                    attempts
                );
            }
        }

        Err(format!(
            "[{}] Failed to validate retry response after {} attempts",
            self.model, attempts
        ))
    }
}

/// Macro to generate model-specific tests
macro_rules! generate_model_test {
    ($model:expr) => {
        #[tokio::test]
        async fn test_find_cat_name() {
            let fixture = Fixture::new(ModelId::new($model));

            let result = fixture
                .test_single_model(|response| response.to_lowercase().contains("juniper"))
                .await;

            assert!(result.is_ok(), "Test failure for {}: {:?}", $model, result);
        }
    };
}

mod anthropic_claude_3_5_sonnet {
    use super::*;
    generate_model_test!("anthropic/claude-3.5-sonnet");
}

mod anthropic_claude_3_7_sonnet {
    use super::*;
    generate_model_test!("anthropic/claude-3.7-sonnet");
}

mod openai_gpt_4o {
    use super::*;
    generate_model_test!("openai/gpt-4o");
}

mod openai_gpt_4o_mini {
    use super::*;
    generate_model_test!("openai/gpt-4o-mini");
}

mod gemini_flash_2_0 {
    use super::*;
    generate_model_test!("google/gemini-2.0-flash-001");
}

mod mistralai_codestral_2501 {
    use super::*;
    generate_model_test!("mistralai/codestral-2501");
}

mod retry_functionality {
    use forge_api::{ConversationId, DispatchEvent};

    use super::*;

    #[tokio::test]
    async fn test_retry_functionality() {
        // Create a fixture with the test model
        let fixture = Fixture::new(ModelId::new("anthropic/claude-3.5-sonnet"));

        // Initialize a conversation with a message
        let initial_message = "What is the capital of France?";
        let (conversation_id, initial_response) = fixture
            .init_conversation_with_message(initial_message)
            .await;

        // Verify initial response contains expected information
        assert!(
            initial_response.to_lowercase().contains("paris"),
            "Initial response should mention Paris"
        );

        // Retry the conversation
        let retry_response = fixture.retry(&conversation_id).await;

        // Verify retry response also contains same message
        assert!(
            retry_response.to_lowercase().contains("paris"),
            "Retry response should mention Paris"
        );

        // Verify that retry used same message
        let conversation = fixture.get_conversation(&conversation_id).await.unwrap();
        let last_user_message = conversation
            .rfind_event(DispatchEvent::USER_TASK_UPDATE)
            .or_else(|| conversation.rfind_event(DispatchEvent::USER_TASK_INIT))
            .unwrap();
        assert_eq!(
            last_user_message.value, initial_message,
            "The last user message should match the initial message"
        );
    }

    #[tokio::test]
    async fn test_retry_with_no_conversation() {
        // Create a fixture with the test model
        let fixture = Fixture::new(ModelId::new("anthropic/claude-3.5-sonnet"));

        // Generate a random conversation ID that doesn't exist
        let conversation_id = ConversationId::generate();

        // Try to retry the non-existent conversation
        let api = fixture.api();
        let result = api.retry(conversation_id).await;

        // Verify that the retry fails with the expected error
        assert!(
            result.is_err(),
            "Retry with non-existent conversation should fail"
        );

        match result {
            Ok(_) => panic!("Expected an error but got success"),
            Err(e) => {
                let err_string = e.to_string();
                assert!(
                    err_string.contains("not found"),
                    "Error should indicate conversation not found, got: {}",
                    err_string
                )
            }
        }
    }

    #[tokio::test]
    async fn test_retry_with_multiple_messages() {
        // Create a fixture with the test model
        let fixture = Fixture::new(ModelId::new("anthropic/claude-3.5-sonnet"));

        // Initialize a conversation with the first message
        let first_message = "What is the capital of France?";
        let (conversation_id, _) = fixture.init_conversation_with_message(first_message).await;

        // Send a second message
        let second_message = "What is the capital of Italy?";
        fixture.send_message(second_message, &conversation_id).await;

        // Retry the conversation
        let retry_response = fixture.retry(&conversation_id).await;

        // Verify retry response contains expected information for the second message
        assert!(
            retry_response.to_lowercase().contains("rome"),
            "Retry response should mention Rome"
        );

        // Verify that retry used the second message
        let conversation = fixture.get_conversation(&conversation_id).await.unwrap();
        let last_user_message = conversation
            .rfind_event(DispatchEvent::USER_TASK_UPDATE)
            .unwrap();
        assert_eq!(
            last_user_message.value, second_message,
            "The last user message should match the second message"
        );
    }

    #[tokio::test]
    async fn test_retry_with_multiple_attempts() {
        // Create a fixture with the test model
        let fixture = Fixture::new(ModelId::new("anthropic/claude-3.5-sonnet"));

        // Initialize a conversation with a message
        let message = "What is the capital of France?";
        let (conversation_id, _) = fixture.init_conversation_with_message(message).await;

        // Test retry with multiple attempts, looking for a specific word
        let result = fixture
            .test_retry_with_attempts(&conversation_id, 3, |response| {
                response.to_lowercase().contains("paris")
            })
            .await;

        assert!(
            result.is_ok(),
            "Retry with multiple attempts should succeed: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_retry_with_different_models() {
        // Test with different models to ensure retry works across models
        let models = ["anthropic/claude-3.5-sonnet", "openai/gpt-4o-mini"];

        for model_name in models {
            // Create a fixture with the test model
            let fixture = Fixture::new(ModelId::new(model_name));

            // Initialize a conversation with a message
            let message = "What is the capital of Germany?";
            let (conversation_id, initial_response) =
                fixture.init_conversation_with_message(message).await;

            // Verify initial response contains expected information
            assert!(
                initial_response.to_lowercase().contains("berlin"),
                "[{}] Initial response should mention Berlin",
                model_name
            );

            // Retry the conversation
            let retry_response = fixture.retry(&conversation_id).await;

            // Verify retry response also contains same information
            assert!(
                retry_response.to_lowercase().contains("berlin"),
                "[{}] Retry response should mention Berlin",
                model_name
            );

            // Verify that retry used same message
            let conversation = fixture.get_conversation(&conversation_id).await.unwrap();
            let last_user_message = conversation
                .rfind_event(DispatchEvent::USER_TASK_UPDATE)
                .or_else(|| conversation.rfind_event(DispatchEvent::USER_TASK_INIT))
                .unwrap();
            assert_eq!(
                last_user_message.value, message,
                "[{}] The last user message should match the initial message",
                model_name
            );
        }
    }
}
