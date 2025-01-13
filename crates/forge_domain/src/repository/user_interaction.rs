use async_trait::async_trait;

/// Port for handling user interaction and prompts
/// This is a port in terms of hexagonal architecture, allowing the application to
/// interact with users through any UI implementation (console, GUI, web, etc.)
#[async_trait]
pub trait UserInteractionPort {
    /// Get text input from the user with a prompt
    async fn get_input(&self, prompt: &str) -> anyhow::Result<String>;
    
    /// Get a confirmation (yes/no) from the user
    async fn get_confirmation(&self, prompt: &str) -> anyhow::Result<bool>;
    
    /// Get a selection from a list of options
    async fn get_selection(&self, prompt: &str, options: &[String]) -> anyhow::Result<String>;
}