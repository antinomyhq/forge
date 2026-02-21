use forge_app::OAuthCallbackServer;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

/// OAuth callback HTTP server implementation
///
/// Provides a lightweight HTTP server for receiving OAuth authorization codes
/// via localhost redirect. Uses raw TCP sockets to avoid heavy framework
/// dependencies.
pub struct ForgeOAuthCallbackServer;

impl ForgeOAuthCallbackServer {
    pub fn new() -> Self {
        Self
    }

    /// Extracts the port number from a redirect URI
    fn extract_port(redirect_uri: &str) -> anyhow::Result<u16> {
        let url = url::Url::parse(redirect_uri)?;
        url.port()
            .ok_or_else(|| anyhow::anyhow!("No port specified in redirect URI"))
    }

    /// Parses query parameters from an HTTP GET request
    fn parse_query_params(request: &str) -> std::collections::HashMap<String, String> {
        // Extract the path line (e.g., "GET /callback?code=abc&state=xyz HTTP/1.1")
        let first_line = request.lines().next().unwrap_or("");
        let parts: Vec<&str> = first_line.split_whitespace().collect();

        if parts.len() < 2 {
            return std::collections::HashMap::new();
        }

        let path = parts[1];

        // Extract query string after '?'
        if let Some(query_start) = path.find('?') {
            let query = &path[query_start + 1..];
            query
                .split('&')
                .filter_map(|pair| {
                    let mut split = pair.splitn(2, '=');
                    let key = split.next()?;
                    let value = split.next()?;
                    Some((
                        urlencoding::decode(key).ok()?.into_owned(),
                        urlencoding::decode(value).ok()?.into_owned(),
                    ))
                })
                .collect()
        } else {
            std::collections::HashMap::new()
        }
    }

    /// Generates an HTML success page
    fn success_page() -> &'static str {
        r#"HTTP/1.1 200 OK
Content-Type: text/html; charset=utf-8
Connection: close

<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Authentication Successful</title>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            display: flex;
            justify-content: center;
            align-items: center;
            height: 100vh;
            margin: 0;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
        }
        .container {
            background: white;
            padding: 3rem;
            border-radius: 1rem;
            box-shadow: 0 20px 60px rgba(0,0,0,0.3);
            text-align: center;
            max-width: 400px;
        }
        h1 {
            color: #667eea;
            margin: 0 0 1rem 0;
            font-size: 2rem;
        }
        p {
            color: #4a5568;
            margin: 0;
            font-size: 1.1rem;
        }
        .checkmark {
            font-size: 4rem;
            color: #48bb78;
            margin-bottom: 1rem;
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="checkmark">✓</div>
        <h1>Authentication Successful!</h1>
        <p>You can close this tab and return to the terminal.</p>
    </div>
</body>
</html>"#
    }

    /// Generates an HTML error page
    fn error_page(message: &str) -> String {
        format!(
            r#"HTTP/1.1 400 Bad Request
Content-Type: text/html; charset=utf-8
Connection: close

<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Authentication Error</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            display: flex;
            justify-content: center;
            align-items: center;
            height: 100vh;
            margin: 0;
            background: linear-gradient(135deg, #f093fb 0%, #f5576c 100%);
        }}
        .container {{
            background: white;
            padding: 3rem;
            border-radius: 1rem;
            box-shadow: 0 20px 60px rgba(0,0,0,0.3);
            text-align: center;
            max-width: 400px;
        }}
        h1 {{
            color: #f5576c;
            margin: 0 0 1rem 0;
            font-size: 2rem;
        }}
        p {{
            color: #4a5568;
            margin: 0;
            font-size: 1.1rem;
        }}
        .error-icon {{
            font-size: 4rem;
            color: #f5576c;
            margin-bottom: 1rem;
        }}
    </style>
</head>
<body>
    <div class="container">
        <div class="error-icon">✗</div>
        <h1>Authentication Error</h1>
        <p>{}</p>
    </div>
</body>
</html>"#,
            message
        )
    }
}

impl Default for ForgeOAuthCallbackServer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl OAuthCallbackServer for ForgeOAuthCallbackServer {
    async fn start_callback_server(
        &self,
        redirect_uri: &str,
        state: &str,
    ) -> anyhow::Result<oneshot::Receiver<String>> {
        let port = Self::extract_port(redirect_uri)?;
        let addr = format!("127.0.0.1:{}", port);

        // Try to bind to the port
        let listener = TcpListener::bind(&addr).await.map_err(|e| {
            anyhow::anyhow!("Failed to bind to port {}: {}. Port may be in use.", port, e)
        })?;

        tracing::debug!("OAuth callback server listening on {}", addr);

        let (tx, rx) = oneshot::channel();
        let expected_state = state.to_string();

        // Spawn a task to handle exactly one connection
        tokio::spawn(async move {
            // Accept exactly one connection
            match listener.accept().await {
                Ok((mut socket, peer_addr)) => {
                    tracing::debug!("Accepted OAuth callback connection from {}", peer_addr);

                    // Read the HTTP request
                    let mut buffer = vec![0u8; 4096];
                    match socket.read(&mut buffer).await {
                        Ok(n) => {
                            let request = String::from_utf8_lossy(&buffer[..n]);
                            let params = Self::parse_query_params(&request);

                            // Validate state parameter
                            if let Some(received_state) = params.get("state") {
                                if received_state != &expected_state {
                                    tracing::error!(
                                        "State mismatch: expected {}, got {}",
                                        expected_state,
                                        received_state
                                    );
                                    let _ = socket
                                        .write_all(
                                            Self::error_page("Invalid state parameter").as_bytes(),
                                        )
                                        .await;
                                    return;
                                }
                            } else {
                                tracing::error!("No state parameter in callback");
                                let _ = socket
                                    .write_all(
                                        Self::error_page("Missing state parameter").as_bytes(),
                                    )
                                    .await;
                                return;
                            }

                            // Extract authorization code
                            if let Some(code) = params.get("code") {
                                tracing::debug!("Received authorization code via callback");

                                // Send success page to browser
                                let _ = socket.write_all(Self::success_page().as_bytes()).await;
                                let _ = socket.flush().await;

                                // Send code through channel
                                let _ = tx.send(code.clone());
                            } else {
                                tracing::error!("No code parameter in callback");
                                let _ = socket
                                    .write_all(
                                        Self::error_page("Missing authorization code").as_bytes(),
                                    )
                                    .await;
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to read from socket: {}", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to accept connection: {}", e);
                }
            }

            // Listener is automatically dropped here, closing the server
            tracing::debug!("OAuth callback server shut down");
        });

        Ok(rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_port() {
        let _server = ForgeOAuthCallbackServer::new();
        let result = ForgeOAuthCallbackServer::extract_port("http://localhost:8080/callback");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 8080);
    }

    #[test]
    fn test_extract_port_no_port() {
        let result = ForgeOAuthCallbackServer::extract_port("http://localhost/callback");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_query_params() {
        let request = "GET /callback?code=abc123&state=xyz789 HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let params = ForgeOAuthCallbackServer::parse_query_params(request);

        assert_eq!(params.get("code"), Some(&"abc123".to_string()));
        assert_eq!(params.get("state"), Some(&"xyz789".to_string()));
    }

    #[test]
    fn test_parse_query_params_url_encoded() {
        let request =
            "GET /callback?code=abc%20123&state=xyz%2B789 HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let params = ForgeOAuthCallbackServer::parse_query_params(request);

        assert_eq!(params.get("code"), Some(&"abc 123".to_string()));
        assert_eq!(params.get("state"), Some(&"xyz+789".to_string()));
    }

    #[tokio::test]
    async fn test_callback_server_binds() {
        let server = ForgeOAuthCallbackServer::new();
        let result = server
            .start_callback_server("http://localhost:18741/callback", "test_state")
            .await;

        // Should successfully bind (or fail if port is occupied, which is expected)
        match result {
            Ok(_receiver) => {
                // Successfully bound - good!
            }
            Err(e) => {
                // Port occupied - also acceptable for this test
                assert!(e.to_string().contains("Port may be in use"));
            }
        }
    }
}
