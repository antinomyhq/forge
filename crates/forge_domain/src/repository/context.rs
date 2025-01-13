use async_trait::async_trait;
use crate::Context;

#[async_trait]
pub trait ContextRepository {
    /// Get the context for the current path
    async fn get_context(&self, path: &str) -> anyhow::Result<Context>;
    
    /// Save context for a path
    async fn save_context(&self, path: &str, context: &Context) -> anyhow::Result<()>;
    
    /// Check if context exists for a path
    async fn has_context(&self, path: &str) -> anyhow::Result<bool>;
    
    /// Delete context for a path
    async fn delete_context(&self, path: &str) -> anyhow::Result<()>;
}