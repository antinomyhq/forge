//! Top-level hook executor — fans [`forge_app::HookExecutorInfra`] method
//! calls out to the four per-kind executors (`shell`, `http`, `prompt`,
//! `agent`).
//!
//! The dispatcher ([`forge_app::hooks::plugin::PluginHookHandler`]) never
//! touches the per-kind executors directly. It holds a single
//! `HookExecutorInfra` trait object and calls `execute_shell` /
//! `execute_http` / `execute_prompt` / `execute_agent` based on the
//! [`forge_domain::HookCommand`] variant that came out of the merged
//! config. This file is the glue that makes that dispatch work.

use std::collections::HashMap;
use std::sync::OnceLock;

use async_trait::async_trait;
use forge_app::{AppConfigService, EnvironmentInfra, HookExecutorInfra, ProviderService, Services};
use forge_domain::{
    AgentHookCommand, Context, ContextMessage, HookExecResult, HookInput, HookOutcome,
    HttpHookCommand, ModelId, PendingHookResult, PromptHookCommand, ResultStreamExt,
    ShellHookCommand,
};

use crate::hook_runtime::agent::ForgeAgentHookExecutor;
use crate::hook_runtime::http::{ForgeHttpHookExecutor, is_url_allowed, map_env_lookup};
use crate::hook_runtime::prompt::ForgePromptHookExecutor;
use crate::hook_runtime::shell::{ForgeShellHookExecutor, PromptHandler};

/// Internal trait object interface for making LLM calls from the hook
/// executor.
///
/// This exists to break the generic type cycle between
/// `ForgeHookExecutor<F>` and `ForgeServices<F>`. The concrete
/// `ForgeServices<F>` implements this trait, and a boxed handle is
/// injected after construction via `ForgeHookExecutor::init_services`.
///
/// Matches the pattern used by `ForgeElicitationDispatcher` for the
/// same cycle-breaking reason.
#[async_trait]
pub trait HookModelService: Send + Sync + 'static {
    /// Execute a single non-streaming LLM call and return the text
    /// content of the response.
    async fn query_model(&self, model_id: &ModelId, context: Context) -> anyhow::Result<String>;
}

/// Blanket implementation: any `Services` aggregate can serve as a
/// `HookModelService` by delegating to `ProviderService::chat`.
#[async_trait]
impl<S: Services + 'static> HookModelService for S {
    async fn query_model(&self, model_id: &ModelId, context: Context) -> anyhow::Result<String> {
        // Resolve the provider for the requested model.
        let provider_id = self.get_default_provider().await?;
        let provider = self.get_provider(provider_id).await?;

        // Make the LLM call.
        let stream = self.chat(model_id, context, provider).await?;
        let message = stream.into_full(false).await?;

        Ok(message.content)
    }
}

/// Concrete implementation of [`HookExecutorInfra`].
///
/// Generic over the environment infrastructure `F` so the HTTP executor
/// can use `F::get_env_var` for header substitution. The three other
/// executors are parameter-free and held as plain values.
///
/// # Late-bound LLM access
///
/// Prompt and agent hooks need to call an LLM, but `ForgeHookExecutor`
/// is constructed before the full Services aggregate exists (it is
/// itself a *field* of `ForgeServices<F>`). To break this cycle, the
/// struct stores an [`OnceLock`]-guarded handle that is populated via
/// [`ForgeHookExecutor::init_services`] after `Arc<ForgeServices<F>>`
/// is constructed — the same pattern used by
/// [`crate::ForgeElicitationDispatcher`].
pub struct ForgeHookExecutor<F> {
    infra: std::sync::Arc<F>,
    shell: ForgeShellHookExecutor,
    http: ForgeHttpHookExecutor,
    prompt: ForgePromptHookExecutor,
    agent: ForgeAgentHookExecutor,
    /// Late-initialized LLM service. Populated by
    /// [`ForgeHookExecutor::init_services`] after the Services
    /// aggregate is constructed. Until init runs, prompt and agent
    /// hooks return an error.
    model_service: OnceLock<std::sync::Arc<dyn HookModelService>>,
}

impl<F> Clone for ForgeHookExecutor<F> {
    fn clone(&self) -> Self {
        Self {
            infra: self.infra.clone(),
            shell: self.shell.clone(),
            http: self.http.clone(),
            prompt: self.prompt.clone(),
            agent: self.agent.clone(),
            model_service: {
                let lock = OnceLock::new();
                if let Some(svc) = self.model_service.get() {
                    let _ = lock.set(svc.clone());
                }
                lock
            },
        }
    }
}

impl<F: std::fmt::Debug> std::fmt::Debug for ForgeHookExecutor<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ForgeHookExecutor")
            .field("infra", &self.infra)
            .field("shell", &self.shell)
            .field("http", &self.http)
            .field("prompt", &self.prompt)
            .field("agent", &self.agent)
            .field("model_service", &self.model_service.get().is_some())
            .finish()
    }
}

impl<F> ForgeHookExecutor<F> {
    /// Creates a new executor with all four per-kind executors in their
    /// default configuration.
    pub fn new(infra: std::sync::Arc<F>) -> Self {
        Self {
            infra,
            shell: ForgeShellHookExecutor::default(),
            http: ForgeHttpHookExecutor::default(),
            prompt: ForgePromptHookExecutor,
            agent: ForgeAgentHookExecutor,
            model_service: OnceLock::new(),
        }
    }

    /// Attach an unbounded sender for async-rewake hook results.
    ///
    /// The sender is forwarded to the shell executor so that background
    /// `asyncRewake` hooks can push [`PendingHookResult`] values into the
    /// queue consumed by the orchestrator between conversation turns.
    pub fn with_async_result_tx(
        mut self,
        tx: tokio::sync::mpsc::UnboundedSender<PendingHookResult>,
    ) -> Self {
        self.shell = self.shell.with_async_result_tx(tx);
        self
    }

    /// Populate the LLM service handle. Must be called from the
    /// `forge_api` / `forge_services` layer immediately after
    /// `Arc::new(ForgeServices::new(...))` returns. First call wins;
    /// subsequent calls are silently ignored per the [`OnceLock`]
    /// contract.
    ///
    /// Until this method runs, prompt and agent hooks return an error
    /// instead of making LLM calls.
    pub fn init_services(&self, services: std::sync::Arc<dyn HookModelService>) {
        let _ = self.model_service.set(services);
    }
}

#[async_trait]
impl<F> HookExecutorInfra for ForgeHookExecutor<F>
where
    F: EnvironmentInfra<Config = forge_config::ForgeConfig> + Send + Sync + 'static,
{
    async fn execute_shell(
        &self,
        config: &ShellHookCommand,
        input: &HookInput,
        env_vars: HashMap<String, String>,
    ) -> anyhow::Result<HookExecResult> {
        self.shell
            .execute(config, input, env_vars, Some(self))
            .await
    }

    async fn execute_http(
        &self,
        config: &HttpHookCommand,
        input: &HookInput,
    ) -> anyhow::Result<HookExecResult> {
        // Check the URL allowlist before executing the HTTP hook.
        if let Ok(forge_config) = self.infra.get_config() {
            let allowed = forge_config.allowed_http_hook_urls.as_deref();
            if !is_url_allowed(&config.url, allowed) {
                tracing::warn!(
                    url = config.url.as_str(),
                    "HTTP hook URL blocked by allowed_http_hook_urls policy"
                );
                return Ok(HookExecResult {
                    outcome: HookOutcome::NonBlockingError,
                    output: None,
                    raw_stdout: String::new(),
                    raw_stderr: format!(
                        "HTTP hook URL '{}' is not in the allowed_http_hook_urls allowlist",
                        config.url
                    ),
                    exit_code: None,
                });
            }
        }

        let mut snapshot = HashMap::new();
        if let Some(allowed) = config.allowed_env_vars.as_ref() {
            for name in allowed {
                if let Some(value) = self.infra.get_env_var(name) {
                    snapshot.insert(name.clone(), value);
                }
            }
        }
        let lookup = map_env_lookup(snapshot);
        self.http.execute(config, input, lookup).await
    }

    async fn execute_prompt(
        &self,
        config: &PromptHookCommand,
        input: &HookInput,
    ) -> anyhow::Result<HookExecResult> {
        self.prompt.execute(config, input, self).await
    }

    async fn execute_agent(
        &self,
        config: &AgentHookCommand,
        input: &HookInput,
    ) -> anyhow::Result<HookExecResult> {
        self.agent.execute(config, input, self).await
    }

    async fn query_model_for_hook(
        &self,
        model_id: &ModelId,
        context: Context,
    ) -> anyhow::Result<String> {
        let svc = self.model_service.get().ok_or_else(|| {
            anyhow::anyhow!(
                "Hook executor LLM service not initialized. \
                 Call init_services() after ForgeServices construction."
            )
        })?;
        svc.query_model(model_id, context).await
    }

    async fn execute_agent_loop(
        &self,
        model_id: &ModelId,
        context: Context,
        max_turns: usize,
        _timeout_secs: u64,
    ) -> anyhow::Result<Option<(bool, Option<String>)>> {
        let svc = self.model_service.get().ok_or_else(|| {
            anyhow::anyhow!(
                "Hook executor LLM service not initialized. \
                 Call init_services() after ForgeServices construction."
            )
        })?;

        let mut ctx = context;

        for turn in 0..max_turns {
            let response_text = svc.query_model(model_id, ctx.clone()).await?;
            let trimmed = response_text.trim();

            // Try to parse as {ok: bool, reason?: string}
            #[derive(serde::Deserialize)]
            struct HookResp {
                ok: bool,
                reason: Option<String>,
            }

            match serde_json::from_str::<HookResp>(trimmed) {
                Ok(resp) => return Ok(Some((resp.ok, resp.reason))),
                Err(_) if turn < max_turns - 1 => {
                    // Add assistant response and retry prompt
                    ctx = ctx
                        .add_message(ContextMessage::assistant(
                            trimmed.to_string(),
                            None,
                            None,
                            None,
                        ))
                        .add_message(ContextMessage::user(
                            "Your response was not valid JSON. Please respond with a JSON object: \
                             {\"ok\": true} or {\"ok\": false, \"reason\": \"Explanation\"}. \
                             You MUST use the exact format."
                                .to_string(),
                            Some(model_id.clone()),
                        ));
                    tracing::debug!(
                        turn,
                        response = %trimmed,
                        "Agent hook response was not valid JSON; retrying"
                    );
                }
                Err(_) => {
                    // Last turn, still invalid
                    tracing::warn!(
                        response = %trimmed,
                        "Agent hook exhausted max turns without valid JSON response"
                    );
                    return Ok(None);
                }
            }
        }

        Ok(None)
    }
}

/// Bridge implementation that delegates prompt requests to the
/// [`HookExecutorInfra::handle_hook_prompt`] default method (or its
/// override) on the containing `ForgeHookExecutor`.
#[async_trait]
impl<F> PromptHandler for ForgeHookExecutor<F>
where
    F: EnvironmentInfra<Config = forge_config::ForgeConfig> + Send + Sync + 'static,
{
    async fn handle_prompt(
        &self,
        request: forge_domain::HookPromptRequest,
    ) -> anyhow::Result<forge_domain::HookPromptResponse> {
        self.handle_hook_prompt(request).await
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use fake::{Fake, Faker};
    use forge_domain::{Environment, HookInputBase, HookInputPayload, HookOutcome};
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    /// Tiny environment stand-in that satisfies `EnvironmentInfra` for the
    /// executor-wiring tests in this module. We only care that the
    /// trait object constructs and that each dispatch path routes to
    /// the correct per-kind executor — the real implementations have
    /// their own unit tests.
    #[derive(Clone)]
    struct StubInfra {
        env_vars: std::collections::HashMap<String, String>,
        config: forge_config::ForgeConfig,
    }

    impl StubInfra {
        fn new() -> Self {
            Self {
                env_vars: std::collections::HashMap::new(),
                config: forge_config::ForgeConfig::default(),
            }
        }

        fn with_env(mut self, key: &str, value: &str) -> Self {
            self.env_vars.insert(key.to_string(), value.to_string());
            self
        }

        fn with_config(mut self, config: forge_config::ForgeConfig) -> Self {
            self.config = config;
            self
        }
    }

    impl EnvironmentInfra for StubInfra {
        type Config = forge_config::ForgeConfig;

        fn get_environment(&self) -> Environment {
            Faker.fake()
        }

        fn get_config(&self) -> anyhow::Result<forge_config::ForgeConfig> {
            Ok(self.config.clone())
        }

        async fn update_environment(
            &self,
            _ops: Vec<forge_domain::ConfigOperation>,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        fn get_env_var(&self, key: &str) -> Option<String> {
            self.env_vars.get(key).cloned()
        }

        fn get_env_vars(&self) -> std::collections::BTreeMap<String, String> {
            self.env_vars
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        }
    }

    fn sample_input() -> HookInput {
        HookInput {
            base: HookInputBase {
                session_id: "sess".to_string(),
                transcript_path: PathBuf::from("/tmp/t.json"),
                cwd: PathBuf::from("/tmp"),
                permission_mode: None,
                agent_id: None,
                agent_type: None,
                hook_event_name: "PreToolUse".to_string(),
            },
            payload: HookInputPayload::PreToolUse {
                tool_name: "Bash".to_string(),
                tool_input: json!({}),
                tool_use_id: "toolu_1".to_string(),
            },
        }
    }

    #[tokio::test]
    async fn test_agent_hook_routes_through_executor() {
        let infra = Arc::new(StubInfra::new());
        let exec = ForgeHookExecutor::new(infra);
        let config = AgentHookCommand {
            prompt: "verify".to_string(),
            condition: None,
            timeout: None,
            model: None,
            status_message: None,
            once: false,
        };
        // Without init_services(), the LLM call fails and agent hook
        // returns a NonBlockingError.
        let result = exec.execute_agent(&config, &sample_input()).await.unwrap();
        assert_eq!(result.outcome, HookOutcome::NonBlockingError);
        assert!(
            result.raw_stderr.contains("Error executing agent hook"),
            "stderr should mention agent hook error: {}",
            result.raw_stderr
        );
    }

    #[tokio::test]
    async fn test_http_hook_header_substitution_uses_env_vars() {
        let infra = Arc::new(StubInfra::new().with_env("API_TOKEN", "test-secret"));
        let _exec = ForgeHookExecutor::new(infra.clone());

        // Build a snapshot the same way execute_http does internally.
        let config = HttpHookCommand {
            url: "http://localhost:9999/unused".to_string(),
            condition: None,
            timeout: None,
            headers: Some({
                let mut h = std::collections::BTreeMap::new();
                h.insert(
                    "Authorization".to_string(),
                    "Bearer ${API_TOKEN}".to_string(),
                );
                h
            }),
            allowed_env_vars: Some(vec!["API_TOKEN".to_string()]),
            status_message: None,
            once: false,
        };

        // Verify the infra resolves the env var correctly.
        assert_eq!(
            infra.get_env_var("API_TOKEN"),
            Some("test-secret".to_string())
        );

        // Build the snapshot HashMap the same way ForgeHookExecutor::execute_http does.
        let mut snapshot = HashMap::new();
        if let Some(allowed) = config.allowed_env_vars.as_ref() {
            for name in allowed {
                if let Some(value) = infra.get_env_var(name) {
                    snapshot.insert(name.clone(), value);
                }
            }
        }
        assert_eq!(
            snapshot.get("API_TOKEN").map(String::as_str),
            Some("test-secret")
        );

        // Verify substitution via the http module's substitute_header_value.
        let lookup = crate::hook_runtime::http::map_env_lookup(snapshot);
        let allowed_refs: Vec<&str> = config
            .allowed_env_vars
            .as_ref()
            .unwrap()
            .iter()
            .map(String::as_str)
            .collect();
        let substituted = crate::hook_runtime::http::substitute_header_value(
            "Bearer ${API_TOKEN}",
            &allowed_refs,
            &lookup,
        );
        assert_eq!(substituted, "Bearer test-secret");
    }

    #[tokio::test]
    async fn test_query_model_for_hook_without_init_returns_error() {
        let infra = Arc::new(StubInfra::new());
        let exec = ForgeHookExecutor::new(infra);
        let model = ModelId::new("test-model");
        let ctx = Context::default();
        let result = exec.query_model_for_hook(&model, ctx).await;
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("not initialized"),
            "error message should mention initialization"
        );
    }

    #[tokio::test]
    async fn test_execute_http_blocks_url_not_in_allowlist() {
        let config = forge_config::ForgeConfig {
            allowed_http_hook_urls: Some(vec!["https://allowed.example.com/*".to_string()]),
            ..Default::default()
        };
        let infra = Arc::new(StubInfra::new().with_config(config));
        let exec = ForgeHookExecutor::new(infra);

        let hook_config = HttpHookCommand {
            url: "https://evil.com/steal".to_string(),
            condition: None,
            timeout: None,
            headers: None,
            allowed_env_vars: None,
            status_message: None,
            once: false,
        };

        let result = exec
            .execute_http(&hook_config, &sample_input())
            .await
            .unwrap();
        assert_eq!(result.outcome, HookOutcome::NonBlockingError);
        assert!(
            result
                .raw_stderr
                .contains("not in the allowed_http_hook_urls"),
            "error should mention allowlist: {}",
            result.raw_stderr
        );
    }

    #[tokio::test]
    async fn test_execute_http_allows_url_when_no_allowlist() {
        // Default config: allowed_http_hook_urls = None (all allowed).
        // We can't actually make the HTTP call succeed (no mock server),
        // but we verify it does NOT get blocked by the allowlist check.
        let infra = Arc::new(StubInfra::new());
        let exec = ForgeHookExecutor::new(infra);

        let hook_config = HttpHookCommand {
            url: "http://127.0.0.1:1/nonexistent".to_string(),
            condition: None,
            timeout: Some(1),
            headers: None,
            allowed_env_vars: None,
            status_message: None,
            once: false,
        };

        let result = exec
            .execute_http(&hook_config, &sample_input())
            .await
            .unwrap();
        // Should NOT be blocked by allowlist; will fail with connection error.
        assert!(
            !result.raw_stderr.contains("allowed_http_hook_urls"),
            "should not be blocked by allowlist"
        );
    }

    #[tokio::test]
    async fn test_execute_http_blocks_all_when_empty_allowlist() {
        let config = forge_config::ForgeConfig {
            allowed_http_hook_urls: Some(vec![]),
            ..Default::default()
        };
        let infra = Arc::new(StubInfra::new().with_config(config));
        let exec = ForgeHookExecutor::new(infra);

        let hook_config = HttpHookCommand {
            url: "https://hooks.example.com/webhook".to_string(),
            condition: None,
            timeout: None,
            headers: None,
            allowed_env_vars: None,
            status_message: None,
            once: false,
        };

        let result = exec
            .execute_http(&hook_config, &sample_input())
            .await
            .unwrap();
        assert_eq!(result.outcome, HookOutcome::NonBlockingError);
        assert!(
            result
                .raw_stderr
                .contains("not in the allowed_http_hook_urls")
        );
    }
}
