mod test_workflow;
use std::env;
use std::path::PathBuf;

use anyhow::Context;
use forge_api::ForgeAPI;
use forge_domain::{AgentMessage, ChatRequest, ChatResponse, Event, ModelId, API};
use tokio_stream::StreamExt;

const MAX_RETRIES: usize = 5;

/// Check if API tests should run based on environment variable
fn should_run_api_tests() -> bool {
    dotenv::dotenv().ok();

    // If FORGE_MOCK is set to true, we can always run the tests
    if let Ok(mock_mode) = env::var("FORGE_MOCK") {
        if mock_mode.to_lowercase() == "true" {
            return true;
        }
    }

    // Otherwise, only run if RUN_API_TESTS is set
    env::var("RUN_API_TESTS").is_ok()
}

/// Check if we should update the mock data
fn should_update_mock_data() -> bool {
    dotenv::dotenv().ok();

    if let Ok(update_mock) = env::var("FORGE_MOCK_UPDATE") {
        update_mock.to_lowercase() == "true"
    } else {
        false
    }
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
        // Set up environment variables for mock mode if not already set
        if env::var("FORGE_MOCK").is_err() {
            env::set_var("FORGE_MOCK", "true");
        }

        // Set up the mock directory if not already set
        if env::var("FORGE_MOCK_DIR").is_err() {
            let mock_dir = std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("tests")
                .join("mock_data");

            // Create the directory if it doesn't exist
            if !mock_dir.exists() {
                std::fs::create_dir_all(&mock_dir).unwrap_or_else(|e| {
                    panic!("Failed to create mock data directory: {}", e);
                });
            }

            env::set_var("FORGE_MOCK_DIR", mock_dir.to_string_lossy().to_string());
        }

        // NOTE: In tests the CWD is not the project root
        ForgeAPI::init(true)
    }

    /// Get model response as text
    async fn get_model_response(&self) -> String {
        let api = self.api();
        let mut workflow = test_workflow::create_test_workflow();

        // in workflow, replace all models with the model we want to test.
        workflow.agents.iter_mut().for_each(|agent| {
            agent.model = Some(self.model.clone());
        });

        // initialize the conversation by storing the workflow.
        let conversation_id = api.init(workflow).await.unwrap().id;
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
                AgentMessage { message: ChatResponse::Text { text, .. }, .. } => Some(text),
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
                    "Skipping API test for {} as neither FORGE_MOCK=true nor RUN_API_TESTS=true is set",
                    $model
                );
                return;
            }

            // If we're in update mode, let the user know
            if should_update_mock_data() {
                println!(
                    "Running test for {} in mock update mode - will record real API responses",
                    $model
                );
            } else {
                println!(
                    "Running test for {} using mock data",
                    $model
                );
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
