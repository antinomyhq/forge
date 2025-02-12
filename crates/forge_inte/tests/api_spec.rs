/*use forge_api::{AgentMessage, ChatRequest, ChatResponse, ModelId, TestAPI, API};
use tokio_stream::StreamExt;

const MAX_RETRIES: usize = 5;

/// Test fixture for API testing that supports parallel model validation
struct Fixture {
    task: String,
    large_model_id: ModelId,
    small_model_id: ModelId,
}

impl Fixture {
    /// Create a new test fixture with the given task
    fn new(task: impl Into<String>, large_model_id: ModelId, small_model_id: ModelId) -> Self {
        Self { task: task.into(), large_model_id, small_model_id }
    }

    /// Get the API service, panicking if not validated
    fn api(&self) -> impl API {
        // NOTE: In tests the CWD is not the project root
        TestAPI::init(
            false,
            self.large_model_id.clone(),
            self.small_model_id.clone(),
        )
    }

    /// Get model response as text
    async fn get_model_response(&self) -> String {
        let request = ChatRequest::new(self.task.clone());
        self.api()
            .chat(request)
            .await
            .unwrap()
            .filter_map(|message| match message.unwrap() {
                AgentMessage { agent, message: ChatResponse::Text(text) } => {
                    // TODO: don't hard code agent id here
                    if agent.as_str() == "developer" {
                        Some(text)
                    } else {
                        None
                    }
                }
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
            let response = self.get_model_response().await;

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
                "There is a cat hidden in the codebase. What is its name? hint: it's present in juniper.md file. You can use any tool at your disposal to find it. Do not ask me any questions.",
                ModelId::new($model),
                ModelId::new($model),
            );

            let result = fixture
                .test_single_model($model, |response| response.to_lowercase().contains("juniper"))
                .await;

            assert!(result.is_ok(), "Test failure for {}: {:?}", $model, result);
        }
    };
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

mod gemini_flash_2_0 {
    use super::*;
    generate_model_test!("google/gemini-2.0-flash-001");
}

mod mistralai_codestral_2501 {
    use super::*;
    generate_model_test!("mistralai/codestral-2501");
}
*/