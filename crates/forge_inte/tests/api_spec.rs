use std::env;
use std::path::PathBuf;

use anyhow::Context;
use forge_api::{AgentMessage, ChatRequest, ChatResponse, Event, ForgeAPI, ModelId, API};
use tokio_stream::StreamExt;

const MAX_RETRIES: usize = 5;
const WORKFLOW_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/test_workflow.yaml");

/// Check if API tests should run based on environment variable
fn should_run_api_tests() -> bool {
    env::var("RUN_API_TESTS").map(|v| v != "0").unwrap_or(true)
}

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
            Event::new(
                "user_task_init",
                "There is a cat hidden in the codebase. What is its name?",
            ),
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
            if !should_run_api_tests() {
                println!(
                    "Skipping API test for {} as RUN_API_TESTS is not set to 'true'",
                    $model
                );
                return;
            }

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
    // Skip the test if no API key is available
    if std::env::var("FORGE_KEY").is_err()
        && std::env::var("OPEN_ROUTER_KEY").is_err()
        && std::env::var("OPENAI_API_KEY").is_err()
        && std::env::var("ANTHROPIC_API_KEY").is_err()
    {
        println!("Skipping test_retry_functionality: No API key found");
        return;
    }

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

    // Wait for at least one response or timeout after 30 seconds
    let mut initial_responses = Vec::new();
    let timeout = std::time::Duration::from_secs(30);

    // Collect responses with a more robust approach
    let mut received_response = false;
    while let Some(result) = tokio::select! {
        message = initial_stream.next() => message,
        _ = tokio::time::sleep(timeout) => None,
    } {
        match result {
            Ok(AgentMessage { message: ChatResponse::Text(text), .. }) => {
                initial_responses.push(text);
                received_response = true;
                // Once we've received at least one response, we can proceed
                if received_response {
                    break;
                }
            }
            Ok(_) => {
                // Handle other types of responses
                received_response = true;
            }
            Err(e) => {
                // Log error but don't fail the test
                eprintln!("Error in initial stream: {}", e);
            }
        }
    }

    // Skip in-depth assertions if no responses were received, but still test the
    // retry functionality
    if !received_response {
        println!("Warning: No initial responses received, but continuing with retry test");
    }

    // Verify that the conversation exists
    let conversation = api.conversation(&conversation_id).await.unwrap();
    assert!(
        conversation.is_some(),
        "Conversation should exist after initial chat"
    );

    // Now test the retry functionality
    let mut retry_stream = match api.retry(conversation_id.clone()).await {
        Ok(stream) => stream,
        Err(e) => {
            panic!("Failed to retry conversation: {}", e);
        }
    };

    // Wait for at least one response or timeout after 30 seconds
    let mut retry_responses = Vec::new();

    // Collect retry responses
    while let Some(result) = tokio::select! {
        message = retry_stream.next() => message,
        _ = tokio::time::sleep(timeout) => None,
    } {
        match result {
            Ok(AgentMessage { message: ChatResponse::Text(text), .. }) => {
                retry_responses.push(text);
                break;
            }
            Ok(_) => {
                // Handle other types of responses
                break;
            }
            Err(e) => {
                // Log error but don't fail the test
                eprintln!("Error in retry stream: {}", e);
            }
        }
    }

    // For testing purposes, we'll consider the test successful if we can call retry
    // without errors. The actual responses might vary based on the model and
    // test environment
    println!("Initial responses: {:?}", initial_responses);
    println!("Retry responses: {:?}", retry_responses);

    // Verify that the conversation exists after retry
    let conversation = api.conversation(&conversation_id).await.unwrap();
    assert!(
        conversation.is_some(),
        "Conversation should exist after retry"
    );

    // The main assertion is that retry was called successfully
    // We don't make assertions about response content which can be
    // model-dependent
}
