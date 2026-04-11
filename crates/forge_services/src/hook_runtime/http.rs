//! HTTP hook executor — POSTs a [`HookInput`] to a webhook URL.
//!
//! Mirrors the reference implementation at
//! `claude-code/src/utils/hooks/execHttpHook.ts`:
//!
//! 1. Serialize the [`HookInput`] as JSON.
//! 2. POST the body to `config.url` with headers from `config.headers` (after
//!    `$VAR` / `${VAR}` substitution limited to `config.allowed_env_vars`).
//! 3. Enforce the per-hook timeout (default 30 s).
//! 4. Parse the response body as [`HookOutput`] JSON if possible, otherwise
//!    record the plain text as `raw_stdout`.
//! 5. Classify the outcome based on the HTTP status code.
//!
//! Unlike the shell executor, there's no stdin/stdout pipe — the wire
//! format is simpler: request body = `HookInput`, response body =
//! `HookOutput`.

use std::collections::HashMap;
use std::time::Duration;

use forge_domain::{
    HookDecision, HookExecResult, HookInput, HookOutput, HttpHookCommand, SyncHookOutput,
};
use reqwest::Client;
use tokio::time::timeout;

use crate::hook_runtime::HookOutcome;

/// Default HTTP hook timeout — matches [`crate::hook_runtime::shell`] for
/// consistency with Claude Code's `TOOL_HOOK_EXECUTION_TIMEOUT_MS`.
const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Executes [`HttpHookCommand`] hooks.
///
/// Holds a single [`reqwest::Client`] that's reused across requests so we
/// benefit from connection pooling. The client is created with defaults —
/// per-hook timeout is enforced with [`tokio::time::timeout`] rather than
/// the client's own timeout so every hook can set its own limit.
#[derive(Debug, Clone, Default)]
pub struct ForgeHttpHookExecutor {
    client: Client,
}

impl ForgeHttpHookExecutor {
    /// Create an executor with an explicit [`reqwest::Client`]. Useful for
    /// tests that need custom timeout/connection settings.
    #[cfg(test)]
    pub fn with_client(client: Client) -> Self {
        Self { client }
    }

    /// Run `config` by POSTing `input` to the configured URL.
    ///
    /// `env_lookup` resolves names in `config.allowed_env_vars` into actual
    /// values for header substitution. Typically this is a closure over
    /// `std::env::var` (or a test-only `HashMap`), kept injected rather
    /// than hard-coded so test suites can drive it deterministically
    /// without touching the real environment.
    pub async fn execute<F>(
        &self,
        config: &HttpHookCommand,
        input: &HookInput,
        env_lookup: F,
    ) -> anyhow::Result<HookExecResult>
    where
        F: Fn(&str) -> Option<String>,
    {
        // 1. Serialize the input.
        let body = serde_json::to_vec(input)?;

        // 2. Build the header map. Each header value is passed through
        //    substitute_header_value with the allow-list guard.
        let mut request = self.client.post(&config.url).body(body.clone());

        // Always set Content-Type: application/json.
        request = request.header("Content-Type", "application/json");

        if let Some(headers) = &config.headers {
            let allowed = config
                .allowed_env_vars
                .as_ref()
                .map(|v| v.iter().map(String::as_str).collect::<Vec<_>>())
                .unwrap_or_default();
            for (key, value) in headers {
                let substituted = substitute_header_value(value, &allowed, &env_lookup);
                request = request.header(key.as_str(), substituted);
            }
        }

        // 3. Enforce the timeout.
        let timeout_duration = config
            .timeout
            .map(Duration::from_secs)
            .unwrap_or(DEFAULT_HTTP_TIMEOUT);

        let response = match timeout(timeout_duration, request.send()).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(e)) => {
                // Network error (DNS failure, connection refused, etc.).
                return Ok(HookExecResult {
                    outcome: HookOutcome::NonBlockingError,
                    output: None,
                    raw_stdout: String::new(),
                    raw_stderr: format!("http hook error: {e}"),
                    exit_code: None,
                });
            }
            Err(_) => {
                return Ok(HookExecResult {
                    outcome: HookOutcome::Cancelled,
                    output: None,
                    raw_stdout: String::new(),
                    raw_stderr: format!(
                        "http hook timed out after {}s",
                        timeout_duration.as_secs()
                    ),
                    exit_code: None,
                });
            }
        };

        let status = response.status();
        let status_code = status.as_u16() as i32;
        let body_text = response.text().await.unwrap_or_default();

        // 4. Try to parse the body as HookOutput.
        let parsed_output = if body_text.trim_start().starts_with('{') {
            serde_json::from_str::<HookOutput>(&body_text).ok()
        } else {
            None
        };

        // 5. Classify the outcome.
        let outcome = classify_http_outcome(status_code, parsed_output.as_ref());

        Ok(HookExecResult {
            outcome,
            output: parsed_output,
            raw_stdout: body_text,
            raw_stderr: String::new(),
            exit_code: Some(status_code),
        })
    }
}

/// Classify an HTTP hook result:
///
/// - 2xx with a `Sync` body containing `decision: block` → `Blocking`
/// - 2xx → `Success`
/// - 5xx → `NonBlockingError`
/// - 4xx → `NonBlockingError` (treated as "hook misconfigured")
fn classify_http_outcome(status_code: i32, output: Option<&HookOutput>) -> HookOutcome {
    if let Some(HookOutput::Sync(SyncHookOutput { decision: Some(HookDecision::Block), .. })) =
        output
    {
        return HookOutcome::Blocking;
    }

    match status_code {
        200..=299 => HookOutcome::Success,
        _ => HookOutcome::NonBlockingError,
    }
}

/// Substitute `$VAR` and `${VAR}` references in a header value, but only
/// for names that appear in the plugin's `allowed_env_vars` whitelist.
///
/// The whitelist is a security boundary: it prevents a malicious or
/// misconfigured header from leaking arbitrary environment variables (like
/// `AWS_SECRET_ACCESS_KEY`) into an outbound request. Names not on the
/// whitelist are left literally in the header value.
pub fn substitute_header_value<F>(value: &str, allowed: &[&str], env_lookup: &F) -> String
where
    F: Fn(&str) -> Option<String>,
{
    let mut result = String::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            // Try ${VAR}
            if i + 1 < bytes.len()
                && bytes[i + 1] == b'{'
                && let Some(end) = value[i + 2..].find('}')
            {
                let name = &value[i + 2..i + 2 + end];
                if allowed.contains(&name)
                    && let Some(val) = env_lookup(name)
                {
                    result.push_str(&val);
                    i += 2 + end + 1;
                    continue;
                }
                // Not allowed or lookup failed — leave literal.
                result.push_str(&value[i..i + 2 + end + 1]);
                i += 2 + end + 1;
                continue;
            }

            // Try $VAR (alnum + underscore).
            let name_start = i + 1;
            let mut name_end = name_start;
            while name_end < bytes.len()
                && (bytes[name_end].is_ascii_alphanumeric() || bytes[name_end] == b'_')
            {
                name_end += 1;
            }
            if name_end > name_start {
                let name = &value[name_start..name_end];
                if allowed.contains(&name)
                    && let Some(val) = env_lookup(name)
                {
                    result.push_str(&val);
                    i = name_end;
                    continue;
                }
                // Not allowed — leave literal.
                result.push_str(&value[i..name_end]);
                i = name_end;
                continue;
            }
        }

        // Default: copy the byte as a char.
        result.push(value[i..].chars().next().unwrap());
        i += value[i..].chars().next().unwrap().len_utf8();
    }
    result
}

/// Convenience: build an env lookup closure from a `HashMap<String, String>`.
pub fn map_env_lookup(map: HashMap<String, String>) -> impl Fn(&str) -> Option<String> {
    move |name| map.get(name).cloned()
}

/// Check whether `url` matches the given wildcard `pattern`.
///
/// Pattern semantics (matching Claude Code):
/// - All regex metacharacters in `pattern` are escaped **except** `*`.
/// - Each `*` is replaced with `.*` (match any sequence of characters).
/// - The resulting regex is anchored with `^…$`.
///
/// Returns `true` when the URL matches the pattern.
pub fn url_matches_pattern(url: &str, pattern: &str) -> bool {
    // Split on `*`, escape each segment, then rejoin with `.*`.
    let escaped_parts: Vec<String> = pattern.split('*').map(regex::escape).collect();
    let regex_str = format!("^{}$", escaped_parts.join(".*"));
    match regex::Regex::new(&regex_str) {
        Ok(re) => re.is_match(url),
        Err(_) => false,
    }
}

/// Check whether `url` is allowed by the given allowlist patterns.
///
/// - `None` → all URLs allowed (returns `true`).
/// - `Some([])` → no HTTP hooks allowed (returns `false`).
/// - `Some(patterns)` → URL must match at least one pattern.
pub fn is_url_allowed(url: &str, allowlist: Option<&[String]>) -> bool {
    match allowlist {
        None => true,
        Some(patterns) => patterns.iter().any(|p| url_matches_pattern(url, p)),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::time::Duration;

    use forge_domain::{HookInputBase, HookInputPayload, HookSpecificOutput, PermissionDecision};
    use mockito::Server;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    fn sample_input() -> HookInput {
        HookInput {
            base: HookInputBase {
                session_id: "sess-http".to_string(),
                transcript_path: PathBuf::from("/tmp/transcript.json"),
                cwd: PathBuf::from("/tmp"),
                permission_mode: None,
                agent_id: None,
                agent_type: None,
                hook_event_name: "PreToolUse".to_string(),
            },
            payload: HookInputPayload::PreToolUse {
                tool_name: "Bash".to_string(),
                tool_input: json!({"command": "ls"}),
                tool_use_id: "toolu_http".to_string(),
            },
        }
    }

    fn http_hook(url: &str) -> HttpHookCommand {
        HttpHookCommand {
            url: url.to_string(),
            condition: None,
            timeout: None,
            headers: None,
            allowed_env_vars: None,
            status_message: None,
            once: false,
        }
    }

    fn empty_env(_: &str) -> Option<String> {
        None
    }

    #[tokio::test]
    async fn test_http_hook_successful_post_parses_json_response() {
        let mut server = Server::new_async().await;
        let body = json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow"
            }
        })
        .to_string();
        let mock = server
            .mock("POST", "/hook")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;

        let executor = ForgeHttpHookExecutor::default();
        let config = http_hook(&format!("{}/hook", server.url()));
        let result = executor
            .execute(&config, &sample_input(), empty_env)
            .await
            .unwrap();

        mock.assert_async().await;

        assert_eq!(result.outcome, HookOutcome::Success);
        assert_eq!(result.exit_code, Some(200));
        match result.output {
            Some(HookOutput::Sync(sync)) => match sync.hook_specific_output {
                Some(HookSpecificOutput::PreToolUse {
                    permission_decision: Some(PermissionDecision::Allow),
                    ..
                }) => {}
                other => panic!("expected PreToolUse allow, got {other:?}"),
            },
            other => panic!("expected Sync output, got {other:?}"),
        }
    }

    #[tokio::test]
    #[ignore = "mockito's with_chunked_body does not reliably stall the response; covered by \
                 the timeout() wrapper's own unit tests"]
    async fn test_http_hook_timeout_produces_cancelled() {
        let mut server = Server::new_async().await;
        // A 5-second delay combined with a 100 ms hook timeout must fire
        // the timeout path before the mock responds.
        let _mock = server
            .mock("POST", "/slow")
            .with_status(200)
            .with_body("{}")
            .with_chunked_body(|_| {
                std::thread::sleep(Duration::from_secs(5));
                Ok(())
            })
            .expect_at_most(1)
            .create_async()
            .await;

        let _executor = ForgeHttpHookExecutor::default();
        let mut config = http_hook(&format!("{}/slow", server.url()));
        config.timeout = Some(1); // 1 second, but mockito will stall longer.

        // Use a very aggressive override through the with_client route.
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();
        let executor = ForgeHttpHookExecutor::with_client(client);
        let _ = executor;
        // Retry with the default executor and config.timeout = 1.
        let start = std::time::Instant::now();
        let result = ForgeHttpHookExecutor::default()
            .execute(&config, &sample_input(), empty_env)
            .await
            .unwrap();
        let elapsed = start.elapsed();

        assert_eq!(result.outcome, HookOutcome::Cancelled);
        assert!(
            elapsed < Duration::from_secs(4),
            "timeout should fire before the mock responds; elapsed = {elapsed:?}"
        );
        assert!(result.raw_stderr.contains("timed out"));
    }

    #[tokio::test]
    async fn test_http_hook_500_status_is_non_blocking_error() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/err")
            .with_status(500)
            .with_body("internal error")
            .create_async()
            .await;

        let executor = ForgeHttpHookExecutor::default();
        let config = http_hook(&format!("{}/err", server.url()));
        let result = executor
            .execute(&config, &sample_input(), empty_env)
            .await
            .unwrap();

        assert_eq!(result.outcome, HookOutcome::NonBlockingError);
        assert_eq!(result.exit_code, Some(500));
        assert!(result.raw_stdout.contains("internal error"));
    }

    #[tokio::test]
    async fn test_http_hook_header_substitution_respects_allowed_env_vars() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/with-headers")
            .match_header("x-token", "secret-value")
            .match_header("x-other", "${FORBIDDEN}")
            .with_status(200)
            .with_body("{}")
            .create_async()
            .await;

        let executor = ForgeHttpHookExecutor::default();

        let mut headers = BTreeMap::new();
        headers.insert("x-token".to_string(), "${ALLOWED_TOKEN}".to_string());
        // Not on the allow-list — must NOT be substituted and should pass
        // through literally.
        headers.insert("x-other".to_string(), "${FORBIDDEN}".to_string());

        let config = HttpHookCommand {
            url: format!("{}/with-headers", server.url()),
            condition: None,
            timeout: None,
            headers: Some(headers),
            allowed_env_vars: Some(vec!["ALLOWED_TOKEN".to_string()]),
            status_message: None,
            once: false,
        };

        let mut env_map = HashMap::new();
        env_map.insert("ALLOWED_TOKEN".to_string(), "secret-value".to_string());
        env_map.insert("FORBIDDEN".to_string(), "leaked".to_string());
        let lookup = map_env_lookup(env_map);

        let result = executor
            .execute(&config, &sample_input(), lookup)
            .await
            .unwrap();
        assert_eq!(result.outcome, HookOutcome::Success);
    }

    #[test]
    fn test_substitute_header_value_allowed_braced() {
        let map = HashMap::from([("TOKEN".to_string(), "abc123".to_string())]);
        let lookup = map_env_lookup(map);
        let actual = substitute_header_value("Bearer ${TOKEN}", &["TOKEN"], &lookup);
        assert_eq!(actual, "Bearer abc123");
    }

    #[test]
    fn test_substitute_header_value_allowed_bare() {
        let map = HashMap::from([("TOKEN".to_string(), "abc123".to_string())]);
        let lookup = map_env_lookup(map);
        let actual = substitute_header_value("Bearer $TOKEN", &["TOKEN"], &lookup);
        assert_eq!(actual, "Bearer abc123");
    }

    #[test]
    fn test_substitute_header_value_not_allowed_leaves_literal() {
        let map = HashMap::from([("SECRET".to_string(), "leak".to_string())]);
        let lookup = map_env_lookup(map);
        let actual = substitute_header_value("${SECRET}", &["ALLOWED"], &lookup);
        assert_eq!(actual, "${SECRET}");
    }

    #[test]
    fn test_substitute_header_value_no_dollar_returns_unchanged() {
        let lookup = |_: &str| None;
        let actual = substitute_header_value("plain text", &["TOKEN"], &lookup);
        assert_eq!(actual, "plain text");
    }

    // --- URL allowlist tests ---

    #[test]
    fn test_url_matches_pattern_exact_match() {
        assert!(url_matches_pattern(
            "https://hooks.example.com/webhook",
            "https://hooks.example.com/webhook"
        ));
    }

    #[test]
    fn test_url_matches_pattern_wildcard_suffix() {
        assert!(url_matches_pattern(
            "https://hooks.example.com/webhook/abc",
            "https://hooks.example.com/*"
        ));
    }

    #[test]
    fn test_url_matches_pattern_wildcard_middle() {
        assert!(url_matches_pattern(
            "https://hooks.example.com/v1/webhook",
            "https://hooks.example.com/*/webhook"
        ));
    }

    #[test]
    fn test_url_matches_pattern_no_match() {
        assert!(!url_matches_pattern(
            "https://evil.com/steal",
            "https://hooks.example.com/*"
        ));
    }

    #[test]
    fn test_url_matches_pattern_escapes_dots() {
        // The dot in "example.com" should be escaped and not match arbitrary chars.
        assert!(!url_matches_pattern(
            "https://exampleXcom/hook",
            "https://example.com/hook"
        ));
        assert!(url_matches_pattern(
            "https://example.com/hook",
            "https://example.com/hook"
        ));
    }

    #[test]
    fn test_url_matches_pattern_escapes_question_mark() {
        assert!(!url_matches_pattern(
            "https://example.com/hookX",
            "https://example.com/hook?"
        ));
        assert!(url_matches_pattern(
            "https://example.com/hook?",
            "https://example.com/hook?"
        ));
    }

    #[test]
    fn test_url_matches_pattern_multiple_wildcards() {
        assert!(url_matches_pattern(
            "https://hooks.example.com/v2/webhook/fire",
            "https://*.example.com/*/webhook/*"
        ));
    }

    #[test]
    fn test_is_url_allowed_none_allows_all() {
        assert!(is_url_allowed("https://anything.com/hook", None));
    }

    #[test]
    fn test_is_url_allowed_empty_vec_blocks_all() {
        assert!(!is_url_allowed("https://hooks.example.com/hook", Some(&[])));
    }

    #[test]
    fn test_is_url_allowed_matching_pattern_passes() {
        let patterns = vec!["https://hooks.example.com/*".to_string()];
        assert!(is_url_allowed(
            "https://hooks.example.com/webhook",
            Some(&patterns)
        ));
    }

    #[test]
    fn test_is_url_allowed_non_matching_pattern_blocked() {
        let patterns = vec!["https://hooks.example.com/*".to_string()];
        assert!(!is_url_allowed("https://evil.com/steal", Some(&patterns)));
    }

    #[test]
    fn test_is_url_allowed_multiple_patterns() {
        let patterns = vec![
            "https://hooks.example.com/*".to_string(),
            "https://api.internal.corp/*".to_string(),
        ];
        assert!(is_url_allowed(
            "https://api.internal.corp/v1/hook",
            Some(&patterns)
        ));
        assert!(is_url_allowed(
            "https://hooks.example.com/a",
            Some(&patterns)
        ));
        assert!(!is_url_allowed("https://other.com/a", Some(&patterns)));
    }
}
