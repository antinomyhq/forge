use std::path::Path;

use forge_app::{APIService, EnvironmentFactory, Service};
use forge_domain::{ChatRequest, ChatResponse, ModelId};
use tokio_stream::StreamExt;

const MAX_RETRIES: usize = 5;

/// Test fixture for API testing that supports parallel model validation
struct Fixture {
    task: String,
}

impl Fixture {
    /// Create a new test fixture with the given task
    fn new(task: impl Into<String>) -> Self {
        Self { task: task.into() }
    }

    /// Get the API service, panicking if not validated
    fn api(&self) -> impl APIService {
        // NOTE: In tests the CWD is not the project root
        let path = Path::new("../../").to_path_buf();
        let path = path.canonicalize().unwrap();
        let env = EnvironmentFactory::new(path).create().unwrap();
        Service::api_service(env).unwrap()
    }

    /// Get model response as text
    async fn get_model_response(&self, model: &str) -> String {
        let request = ChatRequest::new(ModelId::new(model), self.task.clone());
        self.api()
            .chat(request)
            .await
            .unwrap()
            .filter_map(|message| match message.unwrap() {
                ChatResponse::Text(text) => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>()
            .await
            .join("")
            .trim()
            .to_string()
    }

    /// Test single model with retries
    async fn test_single_model(
        &self,
        model: &str,
        check_response: impl Fn(&str) -> bool,
    ) -> Result<(), String> {
        for attempt in 0..MAX_RETRIES {
            let response = self.get_model_response(model).await;

            if check_response(&response) {
                println!(
                    "[{}] Successfully checked response in {} attempts",
                    model,
                    attempt + 1
                );
                return Ok(());
            }

            if attempt < MAX_RETRIES - 1 {
                println!("[{}] Attempt {}/{}", model, attempt + 1, MAX_RETRIES);
            }
        }
        Err(format!("[{}] Failed after {} attempts", model, MAX_RETRIES))
    }
}

/// Macro to generate model-specific tests
macro_rules! generate_model_test {
    ($model:expr) => {
        #[tokio::test]
        async fn test_find_cat_name() {
            let fixture = Fixture::new(
                "There is a cat hidden in the codebase. What is its name? hint: it's present in *.md file, but not in the docs directory. You can use any tool at your disposal to find it. Do not ask me any questions.",
            );

            let result = fixture
                .test_single_model($model, |response| response.to_lowercase().contains("juniper"))
                .await;

            assert!(result.is_ok(), "Test failure for {}: {:?}", $model, result);
        }
    };
}

mod anthropic_claude_3_5_sonnet_beta {
    use super::*;
    generate_model_test!("anthropic/claude-3.5-sonnet:beta");
}

mod openai_gpt_4o_2024_11_20 {
    use super::*;
    generate_model_test!("openai/gpt-4o-2024-11-20");
}

mod anthropic_claude_3_5_sonnet {
    use super::*;
    generate_model_test!("anthropic/claude-3.5-sonnet");
}

mod openai_gpt_4o {
    use super::*;
    generate_model_test!("openai/gpt-4o");
}

mod openai_gpt_4o_mini {
    use super::*;
    generate_model_test!("openai/gpt-4o-mini");
}

mod anthropic_claude_3_sonnet {
    use super::*;
    generate_model_test!("anthropic/claude-3-sonnet");
}
