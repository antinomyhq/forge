use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use forge_domain::{ForgeConfig, ModelId, Workflow};
use forge_services::{FsReadService, Infrastructure};
use merge::Merge;
use serde_json::{Map, Value};

// Import the default configuration
use crate::forge_default::create_default_workflow;

/// Represents the possible sources of a workflow configuration
enum WorkflowSource<'a> {
    /// Explicitly provided path
    ExplicitPath(&'a Path),
    /// Default configuration embedded in the binary
    Default,
    /// Project-specific configuration in the current directory
    ProjectConfig,
}

/// A workflow loader to load the workflow from the given path.
/// It also resolves the internal paths specified in the workflow.
pub struct ForgeLoaderService<F>(Arc<F>);

impl<F> ForgeLoaderService<F> {
    pub fn new(app: Arc<F>) -> Self {
        Self(app)
    }
}

impl<F: Infrastructure> ForgeLoaderService<F> {
    /// Loads the workflow from the given path.
    /// If a path is provided, uses that workflow directly without merging.
    /// If no path is provided:
    ///   - Loads from current directory's forge.yaml merged with defaults (if
    ///     forge.yaml exists)
    ///   - Falls back to embedded default if forge.yaml doesn't exist
    ///
    /// When merging, the project's forge.yaml values take precedence over
    /// defaults.
    ///
    /// After the workflow is loaded, it applies settings from the .forge file
    /// if it exists.
    pub async fn load(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        // Determine the workflow source
        let source = match path {
            Some(path) => WorkflowSource::ExplicitPath(path),
            None if Path::new("forge.yaml").exists() => WorkflowSource::ProjectConfig,
            None => WorkflowSource::Default,
        };

        // Load the workflow based on its source
        let mut workflow = match source {
            WorkflowSource::ExplicitPath(path) => self.load_from_explicit_path(path).await?,
            WorkflowSource::Default => create_default_workflow(),
            WorkflowSource::ProjectConfig => self.load_with_project_config().await?,
        };

        // Apply .forge file settings if it exists
        let current_dir = std::env::current_dir()?;
        let forge_path = current_dir.join(".forge");
        if forge_path.exists() {
            self.apply_forge_file_settings(&mut workflow, &forge_path)
                .await?
        }

        Ok(workflow)
    }

    /// Loads a workflow from a specific file path
    async fn load_from_explicit_path(&self, path: &Path) -> anyhow::Result<Workflow> {
        let content = String::from_utf8(self.0.file_read_service().read(path).await?.to_vec())?;
        let workflow: Workflow = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse workflow from {}", path.display()))?;
        Ok(workflow)
    }

    /// Loads workflow by merging project config with default workflow
    async fn load_with_project_config(&self) -> anyhow::Result<Workflow> {
        let project_path = Path::new("forge.yaml").canonicalize()?;

        let project_content = String::from_utf8(
            self.0
                .file_read_service()
                .read(project_path.as_path())
                .await?
                .to_vec(),
        )?;

        let project_workflow: Workflow =
            serde_yaml::from_str(&project_content).with_context(|| {
                format!(
                    "Failed to parse project workflow: {}",
                    project_path.display()
                )
            })?;

        // Merge workflows with project taking precedence
        let mut merged_workflow = create_default_workflow();
        merged_workflow.merge(project_workflow);

        Ok(merged_workflow)
    }

    /// Applies settings from the .forge file to the workflow
    async fn apply_forge_file_settings(
        &self,
        workflow: &mut Workflow,
        forge_path: &Path,
    ) -> anyhow::Result<()> {
        let content =
            String::from_utf8(self.0.file_read_service().read(forge_path).await?.to_vec())?;
        // Parse the .forge file
        let forge_config: ForgeConfig = serde_yaml::from_str(&content)?;

        // Apply model settings if present
        if let Some(models) = &forge_config.models {
            // Update variables in the workflow
            let variables = workflow.variables.get_or_insert_with(HashMap::new);

            // Get or create models map
            let models_value = variables
                .entry("models".to_string())
                .or_insert_with(|| Value::Object(Map::new()));

            if let Value::Object(models_map) = models_value {
                // Override models from .forge file
                for (role, model_id) in models {
                    models_map.insert(role.clone(), Value::String(model_id.clone()));
                }
            }

            // Apply model references to agents
            self.apply_model_references(workflow, models)?;
        }

        Ok(())
    }

    /// Apply model references to agents in the workflow
    fn apply_model_references(
        &self,
        workflow: &mut Workflow,
        models: &HashMap<String, String>,
    ) -> anyhow::Result<()> {
        // Map of role purposes to model name in the forge config
        let role_mappings = [
            ("advanced_model", "coding"),
            ("standard_model", "default"),
            ("summarization", "summarization"),
        ];

        // Update model references in each agent
        for agent in &mut workflow.agents {
            // Replace model based on agent purpose
            if let Some(model) = &mut agent.model {
                // Skip template variables
                if model.as_str().contains("{{") {
                    continue;
                }

                let _original_model = model.as_str().to_string(); // Create owned string to avoid borrow issues
                let mut matched = false;

                for (workflow_role, config_role) in &role_mappings {
                    // If this agent is using a model that matches a role
                    if model.as_str().contains(workflow_role) {
                        if let Some(new_model) = models.get(*config_role) {
                            *model = ModelId::new(new_model.clone());
                            matched = true;
                            break;
                        }
                    }
                }

                // If no role mapping matched but agent has specific function
                if !matched
                    && (agent.id.to_string().contains("coding")
                        || agent.id.to_string().contains("engineer"))
                    && models.contains_key("coding")
                {
                    let new_model = models.get("coding").unwrap();
                    *model = ModelId::new(new_model.clone());
                }
            }

            // Handle compact model reference if present
            if let Some(compact) = &mut agent.compact {
                // Skip template variables
                if compact.model.as_str().contains("{{") {
                    continue;
                }

                if let Some(summarization_model) = models.get("summarization") {
                    compact.model = ModelId::new(summarization_model.clone());
                }
            }
        }

        Ok(())
    }
}
