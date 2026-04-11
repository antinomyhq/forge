//! Shared test mocks for LLM-based hook executors.

#[cfg(test)]
pub(crate) mod mocks {
    use std::sync::Mutex;

    use forge_app::HookExecutorInfra;
    use forge_domain::{
        AgentHookCommand, Context, HookExecResult, HookInput, HttpHookCommand, ModelId,
        PromptHookCommand, ShellHookCommand,
    };

    /// Mock executor that records the query and returns a canned response.
    pub struct MockLlmExecutor {
        pub response: Mutex<String>,
        pub captured_model: Mutex<Option<String>>,
    }

    impl MockLlmExecutor {
        pub fn with_response(response: &str) -> Self {
            Self {
                response: Mutex::new(response.to_string()),
                captured_model: Mutex::new(None),
            }
        }
    }

    #[async_trait::async_trait]
    impl HookExecutorInfra for MockLlmExecutor {
        async fn execute_shell(
            &self,
            _: &ShellHookCommand,
            _: &HookInput,
            _: std::collections::HashMap<String, String>,
        ) -> anyhow::Result<HookExecResult> {
            unimplemented!()
        }
        async fn execute_http(
            &self,
            _: &HttpHookCommand,
            _: &HookInput,
        ) -> anyhow::Result<HookExecResult> {
            unimplemented!()
        }
        async fn execute_prompt(
            &self,
            _: &PromptHookCommand,
            _: &HookInput,
        ) -> anyhow::Result<HookExecResult> {
            unimplemented!()
        }
        async fn execute_agent(
            &self,
            _: &AgentHookCommand,
            _: &HookInput,
        ) -> anyhow::Result<HookExecResult> {
            unimplemented!()
        }

        async fn query_model_for_hook(
            &self,
            model_id: &ModelId,
            _context: Context,
        ) -> anyhow::Result<String> {
            *self.captured_model.lock().unwrap() = Some(model_id.as_str().to_string());
            Ok(self.response.lock().unwrap().clone())
        }

        async fn execute_agent_loop(
            &self,
            model_id: &ModelId,
            _context: Context,
            _max_turns: usize,
            _timeout_secs: u64,
        ) -> anyhow::Result<Option<(bool, Option<String>)>> {
            *self.captured_model.lock().unwrap() = Some(model_id.as_str().to_string());
            let response = self.response.lock().unwrap().clone();
            #[derive(serde::Deserialize)]
            struct R {
                ok: bool,
                reason: Option<String>,
            }
            match serde_json::from_str::<R>(&response) {
                Ok(r) => Ok(Some((r.ok, r.reason))),
                Err(_) => Ok(None),
            }
        }
    }

    /// Mock that simulates an LLM error.
    pub struct ErrorLlmExecutor;

    #[async_trait::async_trait]
    impl HookExecutorInfra for ErrorLlmExecutor {
        async fn execute_shell(
            &self,
            _: &ShellHookCommand,
            _: &HookInput,
            _: std::collections::HashMap<String, String>,
        ) -> anyhow::Result<HookExecResult> {
            unimplemented!()
        }
        async fn execute_http(
            &self,
            _: &HttpHookCommand,
            _: &HookInput,
        ) -> anyhow::Result<HookExecResult> {
            unimplemented!()
        }
        async fn execute_prompt(
            &self,
            _: &PromptHookCommand,
            _: &HookInput,
        ) -> anyhow::Result<HookExecResult> {
            unimplemented!()
        }
        async fn execute_agent(
            &self,
            _: &AgentHookCommand,
            _: &HookInput,
        ) -> anyhow::Result<HookExecResult> {
            unimplemented!()
        }

        async fn query_model_for_hook(
            &self,
            _model_id: &ModelId,
            _context: Context,
        ) -> anyhow::Result<String> {
            Err(anyhow::anyhow!("provider connection refused"))
        }

        async fn execute_agent_loop(
            &self,
            _: &ModelId,
            _: Context,
            _: usize,
            _: u64,
        ) -> anyhow::Result<Option<(bool, Option<String>)>> {
            Err(anyhow::anyhow!("provider connection refused"))
        }
    }

    /// Mock that hangs forever (for timeout tests).
    pub struct HangingLlmExecutor;

    #[async_trait::async_trait]
    impl HookExecutorInfra for HangingLlmExecutor {
        async fn execute_shell(
            &self,
            _: &ShellHookCommand,
            _: &HookInput,
            _: std::collections::HashMap<String, String>,
        ) -> anyhow::Result<HookExecResult> {
            unimplemented!()
        }
        async fn execute_http(
            &self,
            _: &HttpHookCommand,
            _: &HookInput,
        ) -> anyhow::Result<HookExecResult> {
            unimplemented!()
        }
        async fn execute_prompt(
            &self,
            _: &PromptHookCommand,
            _: &HookInput,
        ) -> anyhow::Result<HookExecResult> {
            unimplemented!()
        }
        async fn execute_agent(
            &self,
            _: &AgentHookCommand,
            _: &HookInput,
        ) -> anyhow::Result<HookExecResult> {
            unimplemented!()
        }

        async fn query_model_for_hook(
            &self,
            _model_id: &ModelId,
            _context: Context,
        ) -> anyhow::Result<String> {
            // Hang forever — let the timeout kick in.
            std::future::pending().await
        }

        async fn execute_agent_loop(
            &self,
            _: &ModelId,
            _: Context,
            _: usize,
            _: u64,
        ) -> anyhow::Result<Option<(bool, Option<String>)>> {
            // Hang forever — let the timeout kick in.
            std::future::pending().await
        }
    }
}
