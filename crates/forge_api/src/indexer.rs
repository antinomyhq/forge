use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::Context;
use forge_app::{EnvironmentService, Infrastructure};
use forge_merger::Merger;
use tracing::{info, warn};

// Global static storage for repository content
static REPO_CONTENT: OnceLock<Mutex<Option<String>>> = OnceLock::new();

/// Get the global repository content storage
fn get_repo_content_storage() -> &'static Mutex<Option<String>> {
    REPO_CONTENT.get_or_init(|| Mutex::new(None))
}

/// Repository indexer service
pub struct ForgeRepoIndexer<F> {
    app: Arc<F>,
}

impl<F> ForgeRepoIndexer<F> {
    pub fn new(app: Arc<F>) -> Self {
        Self {
            app,
        }
    }

    /// Get the indexed repository content
    /// 
    /// Returns the cached repository content if available
    pub fn get_repo_content(&self) -> Option<String> {
        get_repo_content_storage().lock().unwrap().clone()
    }
}

impl<F: Infrastructure> ForgeRepoIndexer<F> {
    /// Generate repository content as a string
    ///
    /// This function creates a merged string containing all repository content
    /// which can be used as a system prompt for agents.
    ///
    /// # Arguments
    ///
    /// * `repo_dir` - The repository directory to index
    pub async fn generate_repo_content(&self, repo_dir: &Path) -> anyhow::Result<String> {
        info!("Indexing repository content from: {}", repo_dir.display());

        // Create a temporary file path for the merger
        // Since Merger requires a file output, we'll use a temp file then read its contents
        let temp_dir = std::env::temp_dir();
        let temp_output = temp_dir.join("repo_mix_temp.txt");
        
        // Create the Merger instance
        let merger = Merger::new(repo_dir, &temp_output).with_separator("```");

        // Process the repository
        merger
            .process()
            .await
            .with_context(|| format!("Failed to merge repository content from {}", repo_dir.display()))?;

        // Read the content back from the file
        let content = tokio::fs::read_to_string(&temp_output)
            .await
            .with_context(|| format!("Failed to read merged content from {}", temp_output.display()))?;
            
        // Clean up the temporary file
        let _ = tokio::fs::remove_file(&temp_output).await;
        
        info!("Repository content successfully indexed from: {}", repo_dir.display());

        Ok(content)
    }

    /// Generate and store repository content in memory during application startup
    pub async fn ensure_repo_content_indexed(&self) -> anyhow::Result<()> {
        // Get the environment
        let env = self.app.environment_service().get_environment();
        
        // Get the current working directory
        let current_dir = Path::new(&env.cwd);
        
        // Generate the repository content
        match self.generate_repo_content(current_dir).await {
            Ok(content) => {
                // Store the content in memory
                let mut repo_content = get_repo_content_storage().lock().unwrap();
                *repo_content = Some(content);
                info!("Successfully indexed repository content in memory");
                Ok(())
            }
            Err(err) => {
                warn!("Failed to generate repository content: {}", err);
                // We don't want to fail the application startup if this fails
                Ok(())
            }
        }
    }
    
    /// Generate repository content for a different path
    /// 
    /// This is useful when switching between projects
    pub async fn update_repo_content(&self, repo_dir: &Path) -> anyhow::Result<()> {
        match self.generate_repo_content(repo_dir).await {
            Ok(content) => {
                // Update the content in memory
                let mut repo_content = get_repo_content_storage().lock().unwrap();
                *repo_content = Some(content);
                info!("Successfully updated repository content in memory for {}", repo_dir.display());
                Ok(())
            }
            Err(err) => {
                warn!("Failed to update repository content: {}", err);
                Err(err)
            }
        }
    }
}