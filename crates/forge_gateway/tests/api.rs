mod api_key_repository;
mod auth_service;

use std::ops::Deref;
use std::sync::Arc;

use api_key_repository::InMemoryApiKeyRepository;
use auth_service::TestAuthorizer;
use axum::body::Body;
use axum::http::{Request, Response};
use axum::Router;
use forge_gateway::data::ApiKey;
use forge_gateway::error::Result;
use forge_gateway::presentation::app;
use forge_gateway::presentation::dto::ApiKeyResponse;
use forge_gateway::service::{ApiKeyService, KeyGeneratorServiceImpl, ProxyService};
use forge_open_router::ProviderBuilder;
use http_body_util::BodyExt;
use tower::ServiceExt;
use uuid::Uuid;

#[derive(Clone)]
struct Server(Router);

impl Deref for Server {
    type Target = Router;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Server {
    pub fn init() -> Self {
        let provider_url = "https://api.openai.com/v1/";
        let provider_key = "abc";
        let provider = ProviderBuilder::from_url(provider_url)
            .with_key(provider_key)
            .build()
            .expect("Failed to build provider");
        let proxy_service = Arc::new(ProxyService::new(provider));

        let api_key_repo = InMemoryApiKeyRepository::from(vec![
            ApiKey {
                id: Uuid::parse_str("8d76a160-a8c7-41dc-9453-7066ed54cff7")
                    .unwrap()
                    .into(),
                user_id: "user-1".to_string(),
                key_name: "key-name-1".to_string(),
                key: "key-1".to_string(),
                created_at: chrono::Utc::now(),
                updated_at: None,
                last_used_at: None,
                expires_at: None,
            },
            ApiKey {
                id: Uuid::parse_str("3d17ca2f-b6c7-423c-a0b2-2fa6fb3ebcb7")
                    .unwrap()
                    .into(),
                user_id: "user-2".to_string(),
                key_name: "key-name-2".to_string(),
                key: "key-2".to_string(),
                created_at: chrono::Utc::now(),
                updated_at: None,
                last_used_at: None,
                expires_at: None,
            },
            ApiKey {
                id: Uuid::parse_str("9311b2db-d4de-4252-92c2-6a5231a5c63e")
                    .unwrap()
                    .into(),
                user_id: "user-1".to_string(),
                key_name: "key-name-3".to_string(),
                key: "key-3".to_string(),
                created_at: chrono::Utc::now(),
                updated_at: None,
                last_used_at: None,
                expires_at: None,
            },
        ]);

        let api_key_repository_impl = Arc::new(api_key_repo);
        let key_gen_service = Arc::new(KeyGeneratorServiceImpl::new(32, "test-api-v0"));
        let api_key_service =
            Arc::new(ApiKeyService::new(api_key_repository_impl, key_gen_service));

        let auth_service = Arc::new(
            TestAuthorizer::default()
                .add("user-1-token", "user-1")
                .add("user-2-token", "user-2")
                .add("user-3-token", "user-3"),
        );
        let router = app(api_key_service, proxy_service, auth_service);
        Self(router)
    }

    pub async fn send_request(&self, request: Request<Body>) -> Result<Response<Body>> {
        Ok(self.0.clone().oneshot(request).await.unwrap())
    }
}

#[cfg(test)]
mod list_keys_tests {
    use super::*;

    #[tokio::test]
    async fn test_list_keys_with_valid_token() {
        let request = Request::builder()
            .uri("/api/v1/user/keys")
            .header("Authorization", "Bearer user-1-token")
            .body(Body::empty())
            .unwrap();
        let response = Server::init().send_request(request).await.unwrap();
        assert_eq!(response.status(), 200);
        let response = serde_json::from_slice::<Vec<ApiKeyResponse>>(
            &response.collect().await.unwrap().to_bytes(),
        )
        .unwrap();

        // verify the response
        assert_eq!(response.len(), 2);
        assert_eq!(response[0].key_name, "key-name-1");
        assert_eq!(response[0].key, "key-1");
        assert_eq!(response[1].key_name, "key-name-3");
        assert_eq!(response[1].key, "key-3");
    }

    #[tokio::test]
    async fn test_list_keys_with_invalid_token() {
        let request = Request::builder()
            .uri("/api/v1/user/keys")
            .header("Authorization", "Bearer user-99-token")
            .body(Body::empty())
            .unwrap();
        let response = Server::init().send_request(request).await.unwrap();
        assert_eq!(response.status(), 401);
    }

    #[tokio::test]
    async fn test_list_keys_without_token() {
        let request = Request::builder()
            .uri("/api/v1/user/keys")
            .body(Body::empty())
            .unwrap();
        let response = Server::init().send_request(request).await.unwrap();
        assert_eq!(response.status(), 401);
    }
}

#[cfg(test)]
mod get_key_tests {
    use super::*;

    #[tokio::test]
    async fn test_get_key_by_id() {
        let request = Request::builder()
            .uri("/api/v1/user/keys/8d76a160-a8c7-41dc-9453-7066ed54cff7")
            .header("Authorization", "Bearer user-1-token")
            .body(Body::empty())
            .unwrap();
        let response = Server::init().send_request(request).await.unwrap();
        assert_eq!(response.status(), 200);
        let response =
            serde_json::from_slice::<ApiKeyResponse>(&response.collect().await.unwrap().to_bytes())
                .unwrap();

        // verify the response
        assert_eq!(response.key_name, "key-name-1");
        assert_eq!(response.key, "key-1");
    }

    #[tokio::test]
    async fn test_get_key_by_id_with_unauth_user_id() {
        // scenario: user-2 is trying to access user-1's key.
        let request = Request::builder()
            .uri("/api/v1/user/keys/8d76a160-a8c7-41dc-9453-7066ed54cff7")
            .header("Authorization", "Bearer user-2-token")
            .body(Body::empty())
            .unwrap();
        let response = Server::init().send_request(request).await.unwrap();
        assert_eq!(response.status(), 404);
    }
}

#[cfg(test)]
mod delete_api_key_tests {
    use axum::http::Method;

    use super::*;

    #[tokio::test]
    async fn test_delete_api_key() {
        let request = Request::builder()
            .uri("/api/v1/user/keys/8d76a160-a8c7-41dc-9453-7066ed54cff7")
            .method(Method::DELETE)
            .header("Authorization", "Bearer user-1-token")
            .body(Body::empty())
            .unwrap();
        let response = Server::init().send_request(request).await.unwrap();
        assert_eq!(response.status(), 204);
    }

    #[tokio::test]
    async fn test_delete_api_key_with_unauth_user() {
        // scenario: user-2 is trying to delete user-1's key.
        let request = Request::builder()
            .uri("/api/v1/user/keys/8d76a160-a8c7-41dc-9453-7066ed54cff7")
            .method(Method::DELETE)
            .header("Authorization", "Bearer user-2-token")
            .body(Body::empty())
            .unwrap();
        let response = Server::init().send_request(request).await.unwrap();
        assert_eq!(response.status(), 404);
    }
}

#[cfg(test)]
mod create_api_key_tests {
    use axum::http::Method;
    use forge_gateway::presentation::dto::CreateApiKeyRequest;

    use super::*;

    #[tokio::test]
    async fn test_create_api_key() {
        let api_creation_request = CreateApiKeyRequest { name: "new-key".to_string() };
        let request = Request::builder()
            .uri("/api/v1/user/keys")
            .method(Method::POST)
            .header("content-type", "application/json")
            .header("Authorization", "Bearer user-2-token")
            .body(serde_json::to_string(&api_creation_request).unwrap().into())
            .unwrap();
        let response = Server::init().send_request(request).await.unwrap();
        assert_eq!(response.status(), 201);
    }

    #[tokio::test]
    async fn test_create_api_key_with_invalid_token() {
        let api_creation_request = CreateApiKeyRequest { name: "new-key".to_string() };
        let request = Request::builder()
            .uri("/api/v1/user/keys")
            .method(Method::POST)
            .header("content-type", "application/json")
            .header("Authorization", "Bearer user-00-token")
            .body(serde_json::to_string(&api_creation_request).unwrap().into())
            .unwrap();
        let response = Server::init().send_request(request).await.unwrap();
        assert_eq!(response.status(), 401);
    }
}
