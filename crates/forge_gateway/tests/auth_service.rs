use axum::http::HeaderMap;
use forge_gateway::{
    error::{Error, Result},
    presentation::AuthUser,
    service::AuthorizeService,
};

// holds a list of (token, associated user) that are considered valid
#[derive(Default)]
pub struct TestAuthorizer(Vec<(String, String)>);

impl TestAuthorizer {
    pub fn add(mut self, token: &str, user: &str) -> Self {
        self.0.push((token.to_string(), user.to_string()));
        self
    }
}

#[async_trait::async_trait]
impl AuthorizeService for TestAuthorizer {
    type Output = AuthUser;
    async fn authorize(&self, headers: &HeaderMap) -> Result<Self::Output> {
        let token = headers
            .get("Authorization")
            .ok_or_else(|| Error::Auth("Authorization header missing".to_string()))?;
        let token = token.to_str().map_err(|e| Error::Auth(e.to_string()))?;

        let token = token.replace("Bearer ", "");
        self.0
            .iter()
            .find(|(t, _)| t == &token)
            .map(|(_, u)| AuthUser { id: u.clone() })
            .ok_or_else(|| Error::Auth("Invalid token".to_string()))
    }
}
