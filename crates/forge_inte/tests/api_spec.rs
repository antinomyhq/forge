mod test_infra;

use std::path::PathBuf;
use std::sync::Arc;

use forge_api::{AgentMessage, ChatRequest, ChatResponse, ForgeAPI, ModelId, API};
use test_infra::TestInfra;
use tokio_stream::StreamExt;

const MAX_RETRIES: usize = 5;
const WORKFLOW_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../forge.toml");

/// Test fixture for API testing that supports parallel model validation
struct Fixture {
    task: String,
    model: ModelId,
    provider_env_name: String,
}

impl Fixture {
    /// Create a new test fixture with the given task
    fn new(task: impl Into<String>, model: ModelId, provider_env_name: String) -> Self {
        Self { task: task.into(), model, provider_env_name }
    }

    /// Get the API service, panicking if not validated
    fn api(&self) -> impl API {
        ForgeAPI::init_with_infra(Arc::new(TestInfra::new(
            true,
            self.provider_env_name.clone(),
        )))
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

        let request = ChatRequest::new(self.task.clone(), conversation_id);
        api.chat(request)
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
    ($model:expr,$provider_env_name:expr) => {
        #[tokio::test]
        async fn test_find_cat_name() {
            let fixture = Fixture::new(
                "There is a cat hidden in the codebase. What is its name? hint: it's present in juniper.md file. You can use any tool at your disposal to find it. Do not ask follow up questions.",
                ModelId::new($model),
                $provider_env_name.to_string(),
            );

            let result = fixture
                .test_single_model(|response| response.to_lowercase().contains("juniper"))
                .await;

            assert!(result.is_ok(), "Test failure for {}: {:?}", $model, result);
        }
    };
}

mod open_router_tests {
    use super::*;
    mod anthropic_claude_3_5_sonnet {
        use super::*;
        generate_model_test!("anthropic/claude-3.5-sonnet", "OPEN_ROUTER_KEY");
    }

    mod openai_gpt_4o {
        use super::*;
        generate_model_test!("openai/gpt-4o", "OPEN_ROUTER_KEY");
    }

    mod openai_gpt_4o_mini {
        use super::*;
        generate_model_test!("openai/gpt-4o-mini", "OPEN_ROUTER_KEY");
    }

    mod gemini_flash_2_0 {
        use super::*;
        generate_model_test!("google/gemini-2.0-flash-001", "OPEN_ROUTER_KEY");
    }

    mod mistralai_codestral_2501 {
        use super::*;
        generate_model_test!("mistralai/codestral-2501", "OPEN_ROUTER_KEY");
    }
}

mod open_ai_tests {
    use super::*;

    mod openai_gpt_4o {
        use super::*;
        generate_model_test!("gpt-4o", "OPEN_AI_KEY");
    }

    mod openai_gpt_4o_mini {
        use super::*;
        generate_model_test!("gpt-4o-mini", "OPEN_AI_KEY");
    }
}

mod anthropic_tests {
    use super::*;

    mod claude_3_5_haiku_20241022 {
        use super::*;
        generate_model_test!("claude-3-5-haiku-20241022", "ANTHROPIC_KEY");
    }

    mod claude_3_5_sonnet_20241022 {
        use super::*;
        generate_model_test!("claude-3-5-sonnet-20241022", "ANTHROPIC_KEY");
    }
}
