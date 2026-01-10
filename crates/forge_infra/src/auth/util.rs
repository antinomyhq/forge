use std::collections::HashMap;

use chrono::Utc;
use forge_domain::{
    AuthCredential, AuthDetails, OAuthConfig, OAuthTokenResponse, OAuthTokens, ProviderId,
};
use oauth2::basic::BasicClient;
use oauth2::{ClientId, RefreshToken, TokenUrl};

use crate::auth::error::Error;

/// Calculate expires_at as Unix timestamp from expires_in seconds
/// Returns None if expires_in is None
pub(crate) fn calculate_expires_at(expires_in: Option<u64>) -> Option<i64> {
    expires_in.map(|seconds| {
        let expires_at = Utc::now() + chrono::Duration::seconds(seconds as i64);
        expires_at.timestamp()
    })
}

/// Convert oauth2 TokenResponse into domain OAuthTokenResponse
pub(crate) fn into_domain<T: oauth2::TokenResponse>(token: T) -> OAuthTokenResponse {
    let expires_in = token.expires_in().map(|d| d.as_secs());
    OAuthTokenResponse {
        access_token: token.access_token().secret().to_string(),
        refresh_token: token.refresh_token().map(|t| t.secret().to_string()),
        expires_in,
        expires_at: calculate_expires_at(expires_in),
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

/// Build HTTP client with custom headers
pub(crate) fn build_http_client(
    custom_headers: Option<&HashMap<String, String>>,
) -> anyhow::Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        // Disable redirects to prevent SSRF vulnerabilities
        .redirect(reqwest::redirect::Policy::none());

    if let Some(headers) = custom_headers {
        let mut header_map = reqwest::header::HeaderMap::new();

        for (key, value) in headers {
            let header_name = reqwest::header::HeaderName::try_from(key.as_str())
                .map_err(|e| anyhow::anyhow!("Invalid header name '{key}': {e}"))?;
            let header_value = value
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid header value for '{key}': {e}"))?;
            header_map.insert(header_name, header_value);
        }

        builder = builder.default_headers(header_map);
    }

    Ok(builder.build()?)
}

/// Build OAuth credential with expiry from provider response only
/// Priority: expires_in > expires_at > None (no expiration)
/// No fallback is applied - we respect what the provider returns
pub(crate) fn build_oauth_credential(
    provider_id: ProviderId,
    token_response: OAuthTokenResponse,
    config: &OAuthConfig,
    _default_expiry: Option<chrono::Duration>, // Unused, kept for API compatibility
) -> anyhow::Result<AuthCredential> {
    let expires_at = if let Some(seconds) = token_response.expires_in {
        // Provider returned expires_in - calculate from now
        Some(Utc::now() + chrono::Duration::seconds(seconds as i64))
    } else if let Some(timestamp) = token_response.expires_at {
        // Provider returned expires_at timestamp - use it directly
        chrono::DateTime::from_timestamp(timestamp, 0)
    } else {
        // Provider didn't return expiration - token doesn't expire
        None
    };
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
) -> anyhow::Result<OAuthTokenResponse> {
    // Build minimal oauth2 client (just need token endpoint)
    let client_id = config
        .client_id
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("client_id is required for token refresh"))?
        .to_string();

    let client = BasicClient::new(ClientId::new(client_id))
        .set_token_uri(TokenUrl::new(config.token_url.to_string())?);

    // Build HTTP client with custom headers
    let http_client = build_http_client(config.custom_headers.as_ref())?;

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
    use super::*;

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
