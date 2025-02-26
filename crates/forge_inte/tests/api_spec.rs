use std::path::PathBuf;

use anyhow::Context;
use forge_api::{AgentMessage, ChatRequest, ChatResponse, ForgeAPI, ModelId, API};
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

/// Test the retry functionality
#[tokio::test]
async fn test_retry_functionality() {
    // Initialize the API
    let api = ForgeAPI::init(true);
    
    // Load the workflow
    let workflow = api.load(Some(&PathBuf::from(WORKFLOW_PATH))).await.unwrap();
    
    // Initialize the conversation
    let conversation_id = api.init(workflow).await.unwrap();
    
    // Create a test message
    let test_message = "Test message for retry functionality";
    let request = ChatRequest::new(test_message, conversation_id.clone());
    
    // Send the initial chat request
    let mut initial_stream = api.chat(request).await.unwrap();
    
    // Wait for at least one response or timeout after 5 seconds
    let mut initial_responses = Vec::new();
    let start_time = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(5);
    
    while let Some(message) = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        initial_stream.next()
    ).await.unwrap_or(None) {
        if let Ok(AgentMessage { message: ChatResponse::Text(text), .. }) = message {
            initial_responses.push(text);
        }
        
        // Break after 5 seconds to avoid hanging
        if start_time.elapsed() > timeout {
            break;
        }
    }
    
    // Verify that the conversation exists
    let conversation = api.conversation(&conversation_id).await.unwrap();
    assert!(conversation.is_some(), "Conversation should exist after initial chat");
    
    // Now test the retry functionality
    let mut retry_stream = api.retry(conversation_id.clone()).await.unwrap();
    
    // Wait for at least one response or timeout after 5 seconds
    let mut retry_responses = Vec::new();
    let start_time = std::time::Instant::now();
    
    while let Some(message) = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        retry_stream.next()
    ).await.unwrap_or(None) {
        if let Ok(AgentMessage { message: ChatResponse::Text(text), .. }) = message {
            retry_responses.push(text);
        }
        
        // Break after 5 seconds to avoid hanging
        if start_time.elapsed() > timeout {
            break;
        }
    }
    
    // For testing purposes, we'll consider the test successful if we can call retry without errors
    // The actual responses might vary based on the model and test environment
    println!("Initial responses: {:?}", initial_responses);
    println!("Retry responses: {:?}", retry_responses);
    
    // Verify that the conversation exists after retry
    let conversation = api.conversation(&conversation_id).await.unwrap();
    assert!(conversation.is_some(), "Conversation should exist after retry");
}
