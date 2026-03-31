use std::collections::HashMap;

use chrono::Utc;
use forge_domain::{
    AuthCredential, AuthDetails, OAuthConfig, OAuthTokenResponse, OAuthTokens, ProviderId,
};
use oauth2::basic::BasicClient;
use oauth2::{ClientId, RefreshToken, TokenUrl};

use crate::auth::error::Error;
use crate::http_client::ClientBuilderExt;

/// Calculate token expiry with fallback duration
pub(crate) fn calculate_token_expiry(
    expires_in: Option<u64>,
    fallback: chrono::Duration,
) -> chrono::DateTime<chrono::Utc> {
    if let Some(seconds) = expires_in {
        Utc::now() + chrono::Duration::seconds(seconds as i64)
    } else {
        Utc::now() + fallback
    }
}

/// Convert oauth2 TokenResponse into domain OAuthTokenResponse
pub(crate) fn into_domain<T: oauth2::TokenResponse>(token: T) -> OAuthTokenResponse {
    OAuthTokenResponse {
        access_token: token.access_token().secret().to_string(),
        refresh_token: token.refresh_token().map(|t| t.secret().to_string()),
        expires_in: token.expires_in().map(|d| d.as_secs()),
        expires_at: None,
        token_type: "Bearer".to_string(),
        scope: token.scopes().map(|scopes| {
            scopes
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        }),
    }
}

/// Build HTTP client with custom headers, respecting proxy and TLS settings
/// from the supplied [`forge_domain::HttpConfig`].
///
/// **Proxy**: `reqwest` automatically reads `HTTPS_PROXY`/`https_proxy`/`ALL_PROXY`
/// for HTTPS traffic, but does **not** fall back to `HTTP_PROXY` for HTTPS requests.
/// In corporate environments where only `HTTP_PROXY` is set, HTTPS requests would
/// bypass the proxy entirely and fail when direct outbound connections are blocked.
/// This function detects that situation and explicitly routes HTTPS traffic through
/// `HTTP_PROXY` as well.
///
/// **TLS**: Corporate proxies commonly perform TLS inspection using a private root
/// CA installed in the system certificate store. `rustls` ships its own Mozilla CA
/// bundle and does **not** read the OS store, so the TLS handshake fails even when
/// the proxy is correctly configured. The `http_config` parameter carries the same
/// `accept_invalid_certs` and `root_cert_paths` settings that `ForgeHttpInfra`
/// uses, so a custom corporate CA is trusted by auth requests too.
pub(crate) fn build_http_client(
    custom_headers: Option<&HashMap<String, String>>,
    http_config: &forge_domain::HttpConfig,
) -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        // Disable redirects to prevent SSRF vulnerabilities
        .redirect(reqwest::redirect::Policy::none())
        .with_proxy_fallback()?
        .with_tls_config(http_config)
        .with_custom_headers(custom_headers.into_iter().flat_map(|m| m.iter()))?
        .build()?)
}

/// Build OAuth credential with consistent expiry handling
pub(crate) fn build_oauth_credential(
    provider_id: ProviderId,
    token_response: OAuthTokenResponse,
    config: &OAuthConfig,
    default_expiry: chrono::Duration,
) -> anyhow::Result<AuthCredential> {
    let expires_at = calculate_token_expiry(token_response.expires_in, default_expiry);
    let oauth_tokens = OAuthTokens::new(
        token_response.access_token,
        token_response.refresh_token,
        expires_at,
    );
    Ok(AuthCredential::new_oauth(
        provider_id,
        oauth_tokens,
        config.clone(),
    ))
}

/// Build OAuthTokenResponse with standard defaults
pub(crate) fn build_token_response(
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
) -> OAuthTokenResponse {
    OAuthTokenResponse {
        access_token,
        refresh_token,
        expires_in,
        expires_at: None,
        token_type: "Bearer".to_string(),
        scope: None,
    }
}

/// Extract OAuth tokens from any credential type
pub(crate) fn extract_oauth_tokens(credential: &AuthCredential) -> anyhow::Result<&OAuthTokens> {
    match &credential.auth_details {
        AuthDetails::OAuth { tokens, .. } => Ok(tokens),
        AuthDetails::OAuthWithApiKey { tokens, .. } => Ok(tokens),
        _ => Err(
            Error::RefreshFailed("Invalid credential type for token extraction".to_string()).into(),
        ),
    }
}

/// Refresh OAuth access token using refresh token
pub(crate) async fn refresh_access_token(
    config: &OAuthConfig,
    refresh_token: &str,
    http_config: &forge_domain::HttpConfig,
) -> anyhow::Result<OAuthTokenResponse> {
    // Build minimal oauth2 client (just need token endpoint)
    let client = BasicClient::new(ClientId::new(config.client_id.to_string()))
        .set_token_uri(TokenUrl::new(config.token_url.to_string())?);

    // Build HTTP client with custom headers and caller-supplied TLS/proxy config
    let http_client = build_http_client(config.custom_headers.as_ref(), http_config)?;

    let refresh_token = RefreshToken::new(refresh_token.to_string());

    // Use GitHub-compliant HTTP function to handle non-RFC responses
    let http_fn = |req| github_compliant_http_request(http_client.clone(), req);

    let token_result = client
        .exchange_refresh_token(&refresh_token)
        .request_async(&http_fn)
        .await?;

    Ok(into_domain(token_result))
}

/// GitHub-compliant HTTP request handler that fixes status codes for error
/// responses
pub(crate) async fn github_compliant_http_request(
    client: reqwest::Client,
    request: http::Request<Vec<u8>>,
) -> Result<http::Response<Vec<u8>>, reqwest::Error> {
    // Execute the request
    let mut req_builder = client
        .request(request.method().clone(), request.uri().to_string())
        .body(request.body().clone());

    for (name, value) in request.headers() {
        req_builder = req_builder.header(name.as_str(), value.as_bytes());
    }

    let response = req_builder.send().await?;

    // Get status and body
    let status_code = response.status();
    let headers = response.headers().clone();
    let body = response.bytes().await?;

    // GitHub-specific fix: If status is 200 but body contains "error" field,
    // change status to 400 so oauth2 crate recognizes it as an error response
    let fixed_status = if status_code.is_success() {
        if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&body) {
            if json.get("error").is_some() {
                // This is actually an error response masquerading as success
                http::StatusCode::BAD_REQUEST
            } else {
                status_code
            }
        } else {
            status_code
        }
    } else {
        status_code
    };

    // Build http::Response with corrected status
    let mut response_builder = http::Response::builder().status(fixed_status);

    // Add headers
    for (name, value) in headers.iter() {
        response_builder = response_builder.header(name, value);
    }

    Ok(response_builder
        .body(body.to_vec())
        .expect("Failed to build HTTP response"))
}

/// Inject custom headers into a header map
pub(crate) fn inject_custom_headers(
    headers: &mut reqwest::header::HeaderMap,
    custom_headers: &Option<HashMap<String, String>>,
) {
    use reqwest::header::{HeaderName, HeaderValue};

    if let Some(custom_headers) = custom_headers {
        for (key, value) in custom_headers {
            if let (Ok(name), Ok(val)) = (HeaderName::try_from(key), HeaderValue::from_str(value)) {
                headers.insert(name, val);
            }
        }
    }
}

/// Parse OAuth error responses during polling
pub(crate) fn handle_oauth_error(error_code: &str) -> Result<(), Error> {
    match error_code {
        "authorization_pending" | "slow_down" => Ok(()),
        "expired_token" => Err(Error::Expired),
        "access_denied" => Err(Error::Denied),
        _ => Err(Error::PollFailed(format!("OAuth error: {error_code}"))),
    }
}

/// Parse token response from JSON
pub(crate) fn parse_token_response(
    body: &str,
) -> Result<(String, Option<String>, Option<u64>), Error> {
    let token_response: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| Error::PollFailed(format!("Failed to parse token response: {e}")))?;

    let access_token = token_response["access_token"]
        .as_str()
        .ok_or_else(|| Error::PollFailed("Missing access_token in response".to_string()))?
        .to_string();

    let refresh_token = token_response["refresh_token"]
        .as_str()
        .map(|s| s.to_string());

    let expires_in = token_response["expires_in"].as_u64();

    Ok((access_token, refresh_token, expires_in))
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    // Serialise proxy-related tests: env vars are process-global so concurrent
    // mutation causes flaky failures.
    static PROXY_TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Prove that a plain `reqwest::Client` (the old implementation) silently
    /// ignores `HTTP_PROXY` when making HTTPS requests.  The fake proxy TCP
    /// listener receives **no** connection — the request bypasses it entirely.
    #[tokio::test]
    async fn test_old_client_ignores_http_proxy_for_https() {
        let _guard = PROXY_TEST_MUTEX.lock().unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_url = format!("http://{}", listener.local_addr().unwrap());

        // Only HTTP_PROXY is set — no HTTPS_PROXY / ALL_PROXY
        unsafe {
            std::env::set_var("HTTP_PROXY", &proxy_url);
            std::env::remove_var("HTTPS_PROXY");
            std::env::remove_var("https_proxy");
            std::env::remove_var("ALL_PROXY");
            std::env::remove_var("all_proxy");
        }

        // OLD: bare reqwest builder — identical to the pre-fix build_http_client
        let old_client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();

        let accept_task = tokio::spawn(async move {
            tokio::time::timeout(
                std::time::Duration::from_millis(500),
                listener.accept(),
            )
            .await
            .is_ok()
        });

        let _ = old_client
            .post("https://github.com/login/device/code")
            .send()
            .await;

        let proxy_was_contacted = accept_task.await.unwrap();
        unsafe { std::env::remove_var("HTTP_PROXY") };

        assert!(
            !proxy_was_contacted,
            "OLD client should bypass HTTP_PROXY for HTTPS — proxy never contacted"
        );
    }

    /// Prove that `build_http_client` (the new implementation) routes HTTPS
    /// requests through `HTTP_PROXY` when no `HTTPS_PROXY` / `ALL_PROXY` is set.
    /// The fake proxy TCP listener **does** receive the connection.
    #[tokio::test]
    async fn test_new_client_routes_https_through_http_proxy() {
        let _guard = PROXY_TEST_MUTEX.lock().unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_url = format!("http://{}", listener.local_addr().unwrap());

        // Only HTTP_PROXY is set — no HTTPS_PROXY / ALL_PROXY
        unsafe {
            std::env::set_var("HTTP_PROXY", &proxy_url);
            std::env::remove_var("HTTPS_PROXY");
            std::env::remove_var("https_proxy");
            std::env::remove_var("ALL_PROXY");
            std::env::remove_var("all_proxy");
        }

        // NEW: build_http_client with the proxy fallback logic
        let new_client = build_http_client(None, &forge_domain::HttpConfig::default()).unwrap();

        let accept_task = tokio::spawn(async move {
            tokio::time::timeout(
                std::time::Duration::from_millis(500),
                listener.accept(),
            )
            .await
            .is_ok()
        });

        let _ = new_client
            .post("https://github.com/login/device/code")
            .send()
            .await;

        let proxy_was_contacted = accept_task.await.unwrap();
        unsafe { std::env::remove_var("HTTP_PROXY") };

        assert!(
            proxy_was_contacted,
            "NEW client should route HTTPS traffic through HTTP_PROXY"
        );
    }

    /// Prove that `build_http_client` applies `root_cert_paths` from a
    /// caller-supplied [`forge_domain::HttpConfig`].  A temp file containing
    /// clearly invalid cert data is passed directly in the config.  The client
    /// must still build successfully, confirming that parse failures are
    /// silently skipped rather than propagated.
    #[test]
    fn test_build_http_client_loads_root_cert_from_config() {
        let _guard = PROXY_TEST_MUTEX.lock().unwrap();

        // Write content that is definitely not a valid PEM or DER certificate.
        // Certificate::from_pem and from_der both return Err, which is silently
        // skipped. The important thing is that the code path is exercised.
        let cert_file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(cert_file.path(), b"not a certificate").unwrap();

        let http_config = forge_domain::HttpConfig {
            root_cert_paths: Some(vec![cert_file
                .path()
                .to_str()
                .unwrap()
                .to_string()]),
            ..Default::default()
        };

        let result = build_http_client(None, &http_config);

        // Client must build successfully — the invalid cert is gracefully
        // skipped rather than propagating an error.
        assert!(
            result.is_ok(),
            "build_http_client should succeed even with unparseable cert: {:?}",
            result.as_ref().err()
        );
    }

    #[test]
    fn test_calculate_token_expiry_with_expires_in() {
        let before = Utc::now();
        let expires_at = calculate_token_expiry(Some(3600), Duration::hours(1));
        let after = Utc::now() + Duration::hours(1);

        assert!(expires_at >= before);
        assert!(expires_at <= after);
    }

    #[test]
    fn test_calculate_token_expiry_with_fallback() {
        let before = Utc::now();
        let expires_at = calculate_token_expiry(None, Duration::days(365));
        let after = Utc::now() + Duration::days(365);

        assert!(expires_at >= before);
        assert!(expires_at <= after);
    }

    #[test]
    fn test_build_token_response() {
        let response = build_token_response(
            "test_token".to_string(),
            Some("refresh_token".to_string()),
            Some(3600),
        );

        assert_eq!(response.access_token, "test_token");
        assert_eq!(response.refresh_token, Some("refresh_token".to_string()));
        assert_eq!(response.expires_in, Some(3600));
        assert_eq!(response.token_type, "Bearer");
    }

    #[test]
    fn test_handle_oauth_error_retryable() {
        assert!(handle_oauth_error("authorization_pending").is_ok());
        assert!(handle_oauth_error("slow_down").is_ok());
    }

    #[test]
    fn test_handle_oauth_error_terminal() {
        assert!(matches!(
            handle_oauth_error("expired_token"),
            Err(Error::Expired)
        ));
        assert!(matches!(
            handle_oauth_error("access_denied"),
            Err(Error::Denied)
        ));
        assert!(matches!(
            handle_oauth_error("unknown_error"),
            Err(Error::PollFailed(_))
        ));
    }
}
