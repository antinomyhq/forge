use std::sync::Arc;

use uuid::Uuid;

use super::key_gen::KeyGeneratorService;
use crate::data::models::{ApiKey, NewApiKey};
use crate::data::repositories::ApiKeyRepository;
use crate::error::{Error, Result};

/// Service for managing API keys
pub struct ApiKeyService {
    repo: Arc<dyn ApiKeyRepository>,
    key_gen: Arc<dyn KeyGeneratorService>,
}

impl ApiKeyService {
    pub fn new(repo: Arc<dyn ApiKeyRepository>, key_gen: Arc<dyn KeyGeneratorService>) -> Self {
        Self { repo, key_gen }
    }

    pub async fn create_api_key(&self, user_id: &str, key_name: &str) -> Result<ApiKey> {
        let key_gen = self.key_gen.generate_key()?;
        let api_key = NewApiKey::new(user_id.to_string(), key_name.to_string(), key_gen);
        self.repo.save(api_key).await
    }

    pub async fn list_api_keys(&self, user_id: &str) -> Result<Vec<ApiKey>> {
        self.repo.list_by_user_id(user_id).await
    }

    pub async fn delete_api_key(&self, user_id: &str, key_id: Uuid) -> Result<()> {
        self.repo.delete_by_key_id(user_id, key_id).await
    }

    pub async fn find_by_api_key(&self, api_key: &str) -> Result<ApiKey> {
        match self.repo.find_by_key(api_key).await? {
            Some(maybe_key) => Ok(maybe_key),
            None => Err(Error::NotFound("API key not found".to_string())),
        }
    }

    pub async fn find_by_key_id(&self, user_id: &str, key_id: Uuid) -> Result<ApiKey> {
        match self.repo.find_by_key_id(user_id, key_id).await? {
            Some(maybe_key) => Ok(maybe_key),
            None => Err(Error::NotFound("API key not found".to_string())),
        }
    }
}
