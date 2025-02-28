use std::sync::Arc;

use axum::body::Body;
use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use serde::Deserialize;

use crate::error::Error;
use crate::service::api_keys::ApiKeyService;
use crate::service::authorization::AuthorizeService;

const X_API_KEY_HEADER: &str = "x-api-key";

#[derive(Debug, Clone, Deserialize)]
pub struct AuthUser {
    pub id: String,
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthUser>()
            .cloned()
            .ok_or_else(|| Error::Auth("User not authenticated".to_string()))
    }
}

/// Middleware to authenticate requests using JWT
pub async fn jwt_auth<T, Out>(
    State(auth_service): State<Arc<T>>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, Error>
where
    T: AuthorizeService<Output = Out> + 'static,
    Out: Into<AuthUser>,
{
    let jwt = auth_service.authorize(request.headers()).await?;
    request.extensions_mut().insert(jwt.into());

    Ok(next.run(request).await)
}

/// Middleware to authenticate requests using custom API key
pub async fn api_key_auth(
    State(service): State<Arc<ApiKeyService>>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, Error> {
    let api_key_str = request
        .headers()
        .get(X_API_KEY_HEADER)
        .ok_or_else(|| Error::Auth("Missing API key".to_string()))?
        .to_str()
        .map_err(|_| Error::Auth("Invalid API key".to_string()))?;

    let api_key = service
        .find_by_api_key(api_key_str)
        .await
        .map_err(|_| Error::Auth("Invalid API key".to_string()))?;
    request.extensions_mut().insert(api_key);
    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize, PartialEq)]
    struct Response {
        error: ErrorResponse,
    }

    #[derive(Deserialize, PartialEq)]
    struct ErrorResponse {
        message: String,
    }

    #[cfg(test)]
    mod api_key_auth_tests {
        use axum::http::{HeaderMap, StatusCode};
        use axum::routing::get;
        use axum::{middleware, Router};
        use http_body_util::BodyExt;
        use tower::ServiceExt;
        use uuid::Uuid;

        use super::*;
        use crate::{ApiKey, KeyGeneratorServiceImpl, MockApiKeyRepository};

        #[tokio::test]
        async fn test_when_x_api_key_header_missing() {
            let repo = Arc::new(MockApiKeyRepository::new());
            let key_gen = Arc::new(KeyGeneratorServiceImpl::new(32, "sk-forge-api-v0"));
            let service = Arc::new(ApiKeyService::new(repo, key_gen));

            async fn handle(headers: HeaderMap) -> String {
                headers["x-api-test"].to_str().unwrap().to_owned()
            }

            let app = Router::new()
                .route("/", get(handle))
                .layer(middleware::from_fn_with_state(service, api_key_auth));

            let res = app
                .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
                .await
                .unwrap();

            assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

            let body = res.collect().await.unwrap().to_bytes();

            let body = serde_json::from_slice::<Response>(&body).unwrap();
            assert_eq!(body.error.message, "Missing API key");
        }

        #[tokio::test]
        async fn test_should_pass_when_header_and_key_is_valid() {
            let mut api_repo = MockApiKeyRepository::new();
            // Configure mock to return a valid API key when queried with "key"
            api_repo
                .expect_find_by_key()
                .with(mockall::predicate::eq("key"))
                .returning(|_| {
                    Box::pin(async {
                        Ok(Some(ApiKey {
                            id: Uuid::new_v4().into(),
                            key: "key".to_string(),
                            key_name: "test key".to_string(),
                            user_id: Uuid::new_v4().to_string(),
                            created_at: chrono::Utc::now(),
                            updated_at: None,
                            last_used_at: None,
                            expires_at: None,
                        }))
                    })
                });

            let repo = Arc::new(api_repo);
            let key_gen = Arc::new(KeyGeneratorServiceImpl::new(32, "sk-forge-api-v0"));
            let service = Arc::new(ApiKeyService::new(repo, key_gen));

            async fn handle() -> &'static str {
                "success"
            }

            let app = Router::new()
                .route("/", get(handle))
                .layer(middleware::from_fn_with_state(service, api_key_auth));

            let res = app
                .oneshot(
                    Request::builder()
                        .uri("/")
                        .header(X_API_KEY_HEADER, "key")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(res.status(), StatusCode::OK);

            let body = res.collect().await.unwrap().to_bytes();
            assert_eq!(body, "success");
        }

        #[tokio::test]
        async fn test_should_pass_when_header_and_key_is_invalid() {
            let mut api_repo = MockApiKeyRepository::new();
            // Configure mock to return a valid API key when queried with "key"
            api_repo
                .expect_find_by_key()
                .with(mockall::predicate::eq("key"))
                .returning(|_| Box::pin(async { Ok(None) }));

            let repo = Arc::new(api_repo);
            let key_gen = Arc::new(KeyGeneratorServiceImpl::new(32, "sk-forge-api-v0"));
            let service = Arc::new(ApiKeyService::new(repo, key_gen));

            async fn handle() -> &'static str {
                "success"
            }

            let app = Router::new()
                .route("/", get(handle))
                .layer(middleware::from_fn_with_state(service, api_key_auth));

            let res = app
                .oneshot(
                    Request::builder()
                        .uri("/")
                        .header(X_API_KEY_HEADER, "key")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
            let body = res.collect().await.unwrap().to_bytes();
            let body = serde_json::from_slice::<Response>(&body).unwrap();
            assert_eq!(body.error.message, "Invalid API key");
        }
    }

    #[cfg(test)]
    mod jwt_auth_tests {
        use axum::http::{HeaderMap, StatusCode};
        use axum::routing::get;
        use axum::{middleware, Router};
        use http_body_util::BodyExt;
        use tower::ServiceExt;
        use uuid::Uuid;

        use super::*;

        // Mock authorization service for testing
        struct MockAuthService;

        #[async_trait::async_trait]
        impl AuthorizeService for MockAuthService {
            type Output = AuthUser;

            async fn authorize(&self, headers: &HeaderMap) -> Result<Self::Output, Error> {
                if let Some(auth_header) = headers.get("Authorization") {
                    if let Ok(auth_str) = auth_header.to_str() {
                        if auth_str == "Bearer valid_token" {
                            return Ok(AuthUser { id: Uuid::new_v4().to_string() });
                        }
                    }
                }
                Err(Error::Auth("Invalid authorization token".to_string()))
            }
        }

        #[tokio::test]
        async fn test_when_authorization_header_missing() {
            let auth_service = Arc::new(MockAuthService);

            async fn handle() -> &'static str {
                "success"
            }

            let app = Router::new()
                .route("/", get(handle))
                .layer(middleware::from_fn_with_state(auth_service, jwt_auth));

            let res = app
                .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
                .await
                .unwrap();

            assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

            let body = res.collect().await.unwrap().to_bytes();
            let body = serde_json::from_slice::<Response>(&body).unwrap();
            assert_eq!(body.error.message, "Invalid authorization token");
        }

        #[tokio::test]
        async fn test_with_valid_jwt_token() {
            let auth_service = Arc::new(MockAuthService);

            async fn handle(user: AuthUser) -> String {
                format!("User ID: {}", user.id)
            }

            let app = Router::new()
                .route("/", get(handle))
                .layer(middleware::from_fn_with_state(auth_service, jwt_auth));

            let res = app
                .oneshot(
                    Request::builder()
                        .uri("/")
                        .header("Authorization", "Bearer valid_token")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(res.status(), StatusCode::OK);

            let body = res.collect().await.unwrap().to_bytes();
            let body_str = String::from_utf8(body.to_vec()).unwrap();
            assert!(body_str.starts_with("User ID:"));
        }

        #[tokio::test]
        async fn test_with_invalid_jwt_token() {
            let auth_service = Arc::new(MockAuthService);

            async fn handle(user: AuthUser) -> String {
                format!("User ID: {}", user.id)
            }

            let app = Router::new()
                .route("/", get(handle))
                .layer(middleware::from_fn_with_state(auth_service, jwt_auth));

            let res = app
                .oneshot(
                    Request::builder()
                        .uri("/")
                        .header("Authorization", "Bearer invalid_token")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

            let body = res.collect().await.unwrap().to_bytes();
            let body = serde_json::from_slice::<Response>(&body).unwrap();
            assert_eq!(body.error.message, "Invalid authorization token");
        }
    }
}
