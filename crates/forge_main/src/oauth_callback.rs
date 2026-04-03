use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

use forge_domain::CodeRequest;
use url::Url;

/// Maximum time to wait for the OAuth browser callback before giving up.
const OAUTH_CALLBACK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

#[derive(Debug, Clone, PartialEq, Eq)]
struct OAuthCallbackPayload {
    code: String,
}

/// Localhost OAuth callback server that waits for a browser redirect and
/// returns the authorization code.
pub(crate) struct LocalhostOAuthCallbackServer {
    redirect_uri: Url,
    task: tokio::task::JoinHandle<anyhow::Result<String>>,
}

impl LocalhostOAuthCallbackServer {
    /// Starts a localhost OAuth callback server when the request uses a
    /// localhost redirect URI.
    ///
    /// Returns `Ok(None)` when the request is not configured for a localhost
    /// callback.
    ///
    /// # Errors
    ///
    /// Returns an error if the localhost redirect URI is invalid or the TCP
    /// listener cannot be bound.
    pub(crate) fn start(request: &CodeRequest) -> anyhow::Result<Option<Self>> {
        let Some(redirect_uri) = localhost_oauth_redirect_uri(request) else {
            return Ok(None);
        };

        let listener = TcpListener::bind(localhost_oauth_bind_addr(&redirect_uri)?)?;
        let callback_path = redirect_uri.path().to_string();
        let expected_state = request.state.to_string();
        let task = tokio::task::spawn_blocking(move || {
            wait_for_localhost_oauth_callback(listener, callback_path, expected_state)
        });

        Ok(Some(Self { redirect_uri, task }))
    }

    /// Returns the redirect URI the callback server is listening on.
    pub(crate) fn redirect_uri(&self) -> &Url {
        &self.redirect_uri
    }

    /// Waits for the browser callback and returns the authorization code.
    ///
    /// # Errors
    ///
    /// Returns an error when the background task fails or the callback request
    /// is invalid.
    pub(crate) async fn wait_for_code(self) -> anyhow::Result<String> {
        self.task
            .await
            .map_err(|e| anyhow::anyhow!("OAuth callback task failed: {e}"))?
    }
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn localhost_oauth_redirect_uri(request: &CodeRequest) -> Option<Url> {
    request
        .oauth_config
        .redirect_uri
        .as_ref()
        .and_then(|uri| Url::parse(uri).ok())
        .filter(|uri| {
            uri.scheme() == "http"
                && matches!(uri.host_str(), Some("localhost") | Some("127.0.0.1"))
                && uri.port().is_some()
        })
}

fn localhost_oauth_bind_addr(redirect_uri: &Url) -> anyhow::Result<String> {
    let host = match redirect_uri.host_str() {
        Some("localhost") => "127.0.0.1",
        Some(host) => host,
        None => anyhow::bail!("OAuth redirect URI is missing a host"),
    };
    let port = redirect_uri
        .port()
        .ok_or_else(|| anyhow::anyhow!("OAuth redirect URI is missing an explicit port"))?;
    Ok(format!("{host}:{port}"))
}

fn oauth_callback_success_page() -> String {
    "<!doctype html><html><head><title>ForgeCode Authorization Successful</title><meta charset=\"utf-8\"></head><body style=\"font-family: -apple-system, BlinkMacSystemFont, sans-serif; display:flex; align-items:center; justify-content:center; min-height:100vh; margin:0; background:#111827; color:#f9fafb;\"><div style=\"text-align:center; padding:2rem;\"><h1 style=\"margin-bottom:0.75rem;\">Authorization Successful</h1><p style=\"color:#d1d5db;\">You can close this window and return to ForgeCode.</p></div></body></html>".to_string()
}

fn oauth_callback_error_page(message: &str) -> String {
    format!(
        "<!doctype html><html><head><title>ForgeCode Authorization Failed</title><meta charset=\"utf-8\"></head><body style=\"font-family: -apple-system, BlinkMacSystemFont, sans-serif; display:flex; align-items:center; justify-content:center; min-height:100vh; margin:0; background:#111827; color:#f9fafb;\"><div style=\"text-align:center; padding:2rem; max-width:42rem;\"><h1 style=\"margin-bottom:0.75rem; color:#fca5a5;\">Authorization Failed</h1><p style=\"color:#d1d5db;\">ForgeCode could not complete sign-in.</p><pre style=\"white-space:pre-wrap; word-break:break-word; margin-top:1rem; padding:1rem; border-radius:0.5rem; background:#1f2937; color:#fca5a5;\">{}</pre></div></body></html>",
        escape_html(message)
    )
}

fn write_http_response(
    stream: &mut TcpStream,
    status_line: &str,
    body: &str,
) -> anyhow::Result<()> {
    let response = format!(
        "HTTP/1.1 {status_line}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()?;
    Ok(())
}

fn parse_oauth_callback_target(
    request_target: &str,
    expected_path: &str,
    expected_state: &str,
) -> anyhow::Result<Option<OAuthCallbackPayload>> {
    let callback_url = Url::parse(&format!("http://localhost{request_target}"))?;
    if callback_url.path() != expected_path {
        return Ok(None);
    }

    let params: HashMap<String, String> = callback_url.query_pairs().into_owned().collect();
    if let Some(error) = params.get("error") {
        let detail = params
            .get("error_description")
            .filter(|value| !value.trim().is_empty())
            .map(|value| format!(": {value}"))
            .unwrap_or_default();
        anyhow::bail!("Authorization failed ({error}{detail})");
    }

    let state = params
        .get("state")
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing OAuth state in callback"))?;
    if state != expected_state {
        anyhow::bail!("OAuth state mismatch. Please try again.");
    }

    let code = params
        .get("code")
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing authorization code in callback"))?
        .to_string();

    Ok(Some(OAuthCallbackPayload { code }))
}

fn wait_for_localhost_oauth_callback(
    listener: TcpListener,
    expected_path: String,
    expected_state: String,
) -> anyhow::Result<String> {
    let deadline = std::time::Instant::now() + OAUTH_CALLBACK_TIMEOUT;
    listener.set_nonblocking(false)?;

    loop {
        let remaining = deadline
            .checked_duration_since(std::time::Instant::now())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Timed out waiting for OAuth callback after {} seconds",
                    OAUTH_CALLBACK_TIMEOUT.as_secs()
                )
            })?;

        let accept_timeout = remaining.min(std::time::Duration::from_secs(5));
        listener.set_nonblocking(true)?;
        let accept_start = std::time::Instant::now();
        let accept_result = loop {
            match listener.accept() {
                Ok(conn) => break Ok(conn),
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    if accept_start.elapsed() >= accept_timeout
                        || std::time::Instant::now() >= deadline
                    {
                        break Err(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "accept timeout",
                        ));
                    }
                }
                Err(e) => break Err(e),
            }
        };

        let (mut stream, _) = match accept_result {
            Ok(conn) => conn,
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(e) => return Err(e.into()),
        };

        let mut buffer = [0u8; 8192];
        let bytes_read = match stream.read(&mut buffer) {
            Ok(n) => n,
            Err(_) => continue,
        };
        if bytes_read == 0 {
            continue;
        }

        let request = String::from_utf8_lossy(&buffer[..bytes_read]);
        let Some(request_line) = request.lines().next() else {
            continue;
        };

        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or_default();
        let request_target = parts.next().unwrap_or_default();

        if method != "GET" {
            let _ = write_http_response(
                &mut stream,
                "405 Method Not Allowed",
                &oauth_callback_error_page("Only GET requests are supported for OAuth callbacks."),
            );
            continue;
        }

        match parse_oauth_callback_target(request_target, &expected_path, &expected_state) {
            Ok(Some(payload)) => {
                let _ = write_http_response(&mut stream, "200 OK", &oauth_callback_success_page());
                return Ok(payload.code);
            }
            Ok(None) => {
                let _ = write_http_response(
                    &mut stream,
                    "404 Not Found",
                    &oauth_callback_error_page(
                        "Received a request for an unexpected callback path.",
                    ),
                );
            }
            Err(err) => {
                let message = err.to_string();
                let _ = write_http_response(
                    &mut stream,
                    "400 Bad Request",
                    &oauth_callback_error_page(&message),
                );
                return Err(err);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    use forge_domain::{OAuthConfig, PkceVerifier, State};

    use super::*;

    fn sample_code_request(authorization_url: &str) -> CodeRequest {
        CodeRequest {
            authorization_url: Url::parse(authorization_url).unwrap(),
            state: State::from("expected-state".to_string()),
            pkce_verifier: Some(PkceVerifier::from("verifier".to_string())),
            oauth_config: OAuthConfig {
                auth_url: Url::parse("https://auth.openai.com/oauth/authorize").unwrap(),
                token_url: Url::parse("https://auth.openai.com/oauth/token").unwrap(),
                client_id: "client-id".to_string().into(),
                scopes: vec!["openid".to_string()],
                redirect_uri: Some("http://localhost:1455/auth/callback".to_string()),
                use_pkce: true,
                token_refresh_url: None,
                custom_headers: None,
                extra_auth_params: None,
            },
        }
    }

    #[test]
    fn extracts_localhost_redirect_uri_from_oauth_request() {
        let request = sample_code_request(
            "https://auth.openai.com/oauth/authorize?client_id=test&redirect_uri=http%3A%2F%2Flocalhost%3A1455%2Fauth%2Fcallback&state=expected-state",
        );

        let redirect_uri = localhost_oauth_redirect_uri(&request).unwrap();

        assert_eq!(redirect_uri.as_str(), "http://localhost:1455/auth/callback");
    }

    #[test]
    fn captures_authorization_code_from_localhost_callback() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let client = thread::spawn(move || {
            let mut stream = TcpStream::connect(addr).unwrap();
            stream
                .write_all(
                    b"GET /auth/callback?code=auth-code&state=expected-state HTTP/1.1\r\nHost: localhost\r\n\r\n",
                )
                .unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            response
        });

        let code = wait_for_localhost_oauth_callback(
            listener,
            "/auth/callback".to_string(),
            "expected-state".to_string(),
        )
        .unwrap();

        let response = client.join().unwrap();

        assert_eq!(code, "auth-code");
        assert!(response.contains("200 OK"));
    }

    #[test]
    fn rejects_localhost_callback_with_mismatched_state() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let client = thread::spawn(move || {
            let mut stream = TcpStream::connect(addr).unwrap();
            stream
                .write_all(
                    b"GET /auth/callback?code=auth-code&state=wrong-state HTTP/1.1\r\nHost: localhost\r\n\r\n",
                )
                .unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            response
        });

        let error = wait_for_localhost_oauth_callback(
            listener,
            "/auth/callback".to_string(),
            "expected-state".to_string(),
        )
        .unwrap_err();

        let response = client.join().unwrap();

        assert!(error.to_string().contains("OAuth state mismatch"));
        assert!(response.contains("400 Bad Request"));
    }
}
