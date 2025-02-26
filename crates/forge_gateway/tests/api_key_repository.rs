use forge_gateway::{
    data::{ApiKey, ApiKeyRepository, NewApiKey},
    error::{Error, Result},
};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Default)]
pub struct InMemoryApiKeyRepository(RwLock<Vec<ApiKey>>);

impl From<Vec<ApiKey>> for InMemoryApiKeyRepository {
    fn from(keys: Vec<ApiKey>) -> Self {
        InMemoryApiKeyRepository(RwLock::new(keys))
    }
}

#[async_trait::async_trait]
impl ApiKeyRepository for InMemoryApiKeyRepository {
    async fn save(&self, api_key: NewApiKey) -> Result<ApiKey> {
        let created_api_key = ApiKey {
            id: Uuid::new_v4().into(),
            user_id: api_key.user_id,
            key_name: api_key.key_name,
            key: api_key.key,
            created_at: chrono::Utc::now(),
            updated_at: None,
            last_used_at: None,
            expires_at: None,
        };

        self.0.write().await.push(created_api_key.clone());
        Ok(created_api_key)
    }
    async fn find_by_key_id(&self, user_id: &str, key_id: Uuid) -> Result<Option<ApiKey>> {
        let api_key = self
            .0
            .read()
            .await
            .iter()
            .find(|k| k.id.to_string() == key_id.to_string() && k.user_id == user_id)
            .cloned();
        Ok(api_key)
    }
    async fn list_by_user_id(&self, user_id: &str) -> Result<Vec<ApiKey>> {
        Ok(self
            .0
            .read()
            .await
            .iter()
            .filter(|k| k.user_id == user_id)
            .cloned()
            .collect())
    }
    async fn delete_by_key_id(&self, user_id: &str, key_id: Uuid) -> Result<()> {
        let mut keys = self.0.write().await;
        let index = keys
            .iter()
            .position(|k| k.id.to_string() == key_id.to_string() && k.user_id == user_id);
        if let Some(index) = index {
            keys.remove(index);
            Ok(())
        } else {
            Err(Error::NotFound("Key not found".to_string()))
        }
    }
    async fn find_by_key(&self, key: &str) -> Result<Option<ApiKey>> {
        let api_key = self.0.read().await.iter().find(|k| k.key == key).cloned();
        Ok(api_key)
    }
}
