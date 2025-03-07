use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::{env, fs};

use anyhow::{anyhow, Context, Result};
use forge_api::{AgentMessage, ChatRequest, ChatResponse, Event, ForgeAPI, ModelId, API};
use once_cell::sync::Lazy;
use tokio_stream::StreamExt;

const MAX_RETRIES: usize = 5;
const WORKFLOW_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/test_workflow.yaml");
const MOCK_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/mocks");

/// Cache for mock data to avoid reading from disk repeatedly during tests
static MOCK_CACHE: Lazy<Mutex<HashMap<String, String>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// Enum to control how tests are run
#[derive(Debug, Clone, Copy, PartialEq)]
enum MockMode {
    /// Use real API calls
    Real,
    /// Use mock data, fail if mock doesn't exist
    Mock,
    /// Use real API calls and update mocks
    Update,
}

impl MockMode {
    /// Get the current mock mode from environment variables
    fn from_env() -> Self {
        match env::var("FORGE_MOCK_MODE")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "real" => Self::Real,
            "update" => Self::Update,
            _ => Self::Mock,
        }
    }

    /// Check if we should use mocks
    fn use_mocks(&self) -> bool {
        matches!(self, Self::Mock)
    }

    /// Check if we should update mocks
    fn update_mocks(&self) -> bool {
        matches!(self, Self::Update)
    }
}
/*
/// Check if API tests should run based on environment variable
fn should_run_api_tests() -> bool {
    true
    // env::var("RUN_API_TESTS").map(|v| v != "0").unwrap_or(true)
}*/

/// Test fixture for API testing that supports parallel model validation
struct Fixture {
    model: ModelId,
    mock_mode: MockMode,
    #[allow(dead_code)] // The guard is kept alive by being held in the struct
    _guard: forge_tracker::Guard,
}

impl Fixture {
    /// Create a new test fixture with the given task
    fn new(model: ModelId) -> Self {
        // Ensure mock directory exists
        if let Err(e) = fs::create_dir_all(MOCK_DIR) {
            eprintln!("Warning: Failed to create mock directory: {}", e);
        }

        Self {
            model,
            mock_mode: MockMode::from_env(),
            _guard: forge_tracker::init_tracing(PathBuf::from(".")).unwrap(),
        }
    }

    /// Get the API service, panicking if not validated
    fn api(&self) -> impl API {
        // NOTE: In tests the CWD is not the project root
        ForgeAPI::init(true)
    }

    /// Generate a standardized mock file path for a model
    fn mock_path(&self) -> PathBuf {
        let model_name = self.model.to_string().replace('/', "_");
        PathBuf::from(MOCK_DIR).join(format!("{}.mock.txt", model_name))
    }

    /// Load mock response from the file system
    fn load_mock(&self) -> Result<String> {
        let path = self.mock_path();
        let model_key = self.model.to_string();

        // Try to get from cache first
        if let Some(cached) = MOCK_CACHE.lock().unwrap().get(&model_key) {
            return Ok(cached.clone());
        }

        // Read from file
        if path.exists() {
            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read mock data from {}", path.display()))?;

            // Update cache
            MOCK_CACHE
                .lock()
                .unwrap()
                .insert(model_key, content.clone());

            Ok(content)
        } else {
            Err(anyhow!("Mock file does not exist for {}", self.model))
        }
    }

    /// Save mock response to the file system
    fn save_mock(&self, response: &str) -> Result<()> {
        let path = self.mock_path();

        // Update cache first
        MOCK_CACHE
            .lock()
            .unwrap()
            .insert(self.model.to_string(), response.to_string());

        // Save to file
        fs::write(&path, response)
            .with_context(|| format!("Failed to write mock data to {}", path.display()))?;

        println!("Updated mock for {} at {}", self.model, path.display());
        Ok(())
    }

    /// Get model response as text
    async fn get_model_response(&self) -> String {
        // Check if we should use mocks
        if self.mock_mode.use_mocks() {
            match self.load_mock() {
                Ok(mock) => return mock,
                Err(e) => {
                    panic!("Failed to load mock for {}: {}. Set FORGE_MOCK_MODE=update to create mocks.", 
                        self.model, e);
                }
            }
        }

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

        let response = api
            .chat(request)
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
            .to_string();

        // Update mock if requested
        if self.mock_mode.update_mocks() {
            if let Err(e) = self.save_mock(&response) {
                eprintln!("Warning: Failed to update mock for {}: {}", self.model, e);
            }
        }

        response
    }

    /// Test single model with retries
    async fn test_single_model(&self, check_response: impl Fn(&str) -> bool) -> Result<(), String> {
        // If using mocks, don't retry
        if self.mock_mode.use_mocks() {
            let response = self.get_model_response().await;
            if check_response(&response) {
                println!("[{}] Mock check passed", self.model);
                return Ok(());
            }
            return Err(format!("[{}] Mock check failed", self.model));
        }

        // For real calls or updates, use retry logic
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
            /*            if !should_run_api_tests() {
                println!(
                    "Skipping API test for {} as RUN_API_TESTS is not set to 'true'",
                    $model
                );
                return;
            }*/

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
