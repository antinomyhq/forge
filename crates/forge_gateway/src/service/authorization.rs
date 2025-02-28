use axum::http::HeaderMap;
use clerk_rs::validators::authorizer::{ClerkAuthorizer, ClerkError, ClerkJwt};
use clerk_rs::validators::axum::AxumClerkRequest;

use crate::{AuthUser, Error, Result};

#[async_trait::async_trait]
pub trait AuthorizeService: Send + Sync {
    type Output;
    async fn authorize(&self, headers: &HeaderMap) -> Result<Self::Output>;
}

pub struct ClerkAuthorizeService(ClerkAuthorizer);

impl ClerkAuthorizeService {
    pub fn new(authorizer: ClerkAuthorizer) -> Self {
        Self(authorizer)
    }
}

impl From<ClerkJwt> for AuthUser {
    fn from(jwt: ClerkJwt) -> Self {
        Self { id: jwt.sub }
    }
}

#[async_trait::async_trait]
impl AuthorizeService for ClerkAuthorizeService {
    type Output = ClerkJwt;

    async fn authorize(&self, headers: &HeaderMap) -> Result<Self::Output> {
        let jwt = self
            .0
            .authorize(&AxumClerkRequest { headers: headers.clone() })
            .await
            .map_err(|e| match e {
                ClerkError::Unauthorized(msg) => Error::Auth(msg),
                ClerkError::InternalServerError(msg) => Error::Service(msg),
            })?;
        Ok(jwt)
    }
}
