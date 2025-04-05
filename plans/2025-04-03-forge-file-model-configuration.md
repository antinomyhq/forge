# Model Configuration via `.forge` File (Implementation Plan)

## Objective

Create a complementary configuration mechanism for Forge that allows users to configure models via simple commands, storing settings in a `.forge` file. This approach will provide a command-based interface for common configuration tasks while maintaining compatibility with existing `forge.yaml` configuration.

## Configuration Priority

The configuration system will use the following priority order (highest to lowest):

1. Explicitly specified workflow via the `-w` flag
2. Model settings from the `.forge` file
3. Settings from the `forge.yaml` file
4. Default configuration from embedded `forge.default.yaml`

This layered approach allows different levels of configuration to coexist, with more specific settings taking precedence over general ones.

## Implementation Plan

### 1. Create ForgeConfig Structure in forge_domain

Add a new file `forge_config.rs` in the `crates/forge_domain/src/` directory to define the configuration structure:

```rust
// crates/forge_domain/src/forge_config.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct ForgeConfig {
    /// Model mappings for different roles
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<HashMap<String, String>>,
    
    // Can be extended with other settings in the future
}
```

Update the module exports in `crates/forge_domain/src/lib.rs`:

```rust
// Add to existing exports
mod forge_config;
pub use forge_config::ForgeConfig;
```

### 2. Create Command Handling in forge_main

Add a new file `crates/forge_main/src/commands/config.rs` to implement the configuration commands:

```rust
// crates/forge_main/src/commands/config.rs

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use anyhow::Result;

use crate::console::CONSOLE;
use forge_domain::ForgeConfig;

/// Command handler for /set-coding-model
pub fn handle_set_coding_model(model_name: &str) -> Result<()> {
    update_forge_file_model("coding", model_name)
}

/// Command handler for /set-summarization-model
pub fn handle_set_summarization_model(model_name: &str) -> Result<()> {
    update_forge_file_model("summarization", model_name)
}

/// Command handler for /set-default-model
pub fn handle_set_default_model(model_name: &str) -> Result<()> {
    update_forge_file_model("default", model_name)
}

/// Helper function to update a model in the .forge file
fn update_forge_file_model(model_type: &str, model_name: &str) -> Result<()> {
    // Load existing .forge file or create new one
    let mut config = load_forge_config()?;
    
    // Update the specified model
    let models = config.models.get_or_insert_with(HashMap::new);
    models.insert(model_type.to_string(), model_name.to_string());
    
    // Save the updated configuration
    save_forge_config(&config)?;
    
    // Use the global CONSOLE instance for output
    CONSOLE.writeln(format!("{} model set to: {}", model_type, model_name))?;
    Ok(())
}

/// Load configuration from .forge file
pub fn load_forge_config() -> Result<ForgeConfig> {
    let path = Path::new(".forge");
    
    if !path.exists() {
        // Return empty config if file doesn't exist
        return Ok(ForgeConfig::default());
    }
    
    let content = fs::read_to_string(path)?;
    
    // Parse YAML content
    Ok(serde_yaml::from_str(&content)?)
}

/// Save configuration to .forge file
pub fn save_forge_config(config: &ForgeConfig) -> Result<()> {
    let path = Path::new(".forge");
    
    // Convert to YAML
    let content = serde_yaml::to_string(config)?;
    
    // Write to file
    Ok(fs::write(path, content)?)
}
```

Update the module structure in `crates/forge_main/src/commands/mod.rs` (create if doesn't exist):

```rust
// crates/forge_main/src/commands/mod.rs
pub mod config;
pub use config::*;
```

### 3. Register Commands with ForgeCommandManager

Update the `ForgeCommandManager` in `crates/forge_main/src/model.rs` to register the new commands:

```rust
// Add to the default_commands method in ForgeCommandManager
fn default_commands() -> Vec<ForgeCommand> {
    let mut commands = vec![
        // ...existing commands
        
        // Add new model configuration commands
        ForgeCommand {
            name: "/set-coding-model".to_string(),
            description: "Set the model used for coding tasks".to_string(),
            value: "set_coding_model".to_string(),
        },
        ForgeCommand {
            name: "/set-summarization-model".to_string(),
            description: "Set the model used for summarization tasks".to_string(),
            value: "set_summarization_model".to_string(),
        },
        ForgeCommand {
            name: "/set-default-model".to_string(),
            description: "Set the default model".to_string(),
            value: "set_default_model".to_string(),
        },
    ];
    
    commands
}

// Add to the parse method in ForgeCommandManager to handle the new commands
pub fn parse(&self, text: &str) -> Result<Command, Error> {
    // ...existing parsing logic
    
    if let Some(command) = self.find(text) {
        match command.value.as_str() {
            // ...existing command cases
            
            "set_coding_model" => {
                let parts: Vec<&str> = text.split_whitespace().collect();
                if parts.len() < 2 {
                    return Err(Error::InvalidCommandFormat);
                }
                let model = parts[1..].join(" ");
                crate::commands::handle_set_coding_model(&model)?;
                Ok(Command::Custom(Event::new("set_coding_model", model)))
            },
            "set_summarization_model" => {
                let parts: Vec<&str> = text.split_whitespace().collect();
                if parts.len() < 2 {
                    return Err(Error::InvalidCommandFormat);
                }
                let model = parts[1..].join(" ");
                crate::commands::handle_set_summarization_model(&model)?;
                Ok(Command::Custom(Event::new("set_summarization_model", model)))
            },
            "set_default_model" => {
                let parts: Vec<&str> = text.split_whitespace().collect();
                if parts.len() < 2 {
                    return Err(Error::InvalidCommandFormat);
                }
                let model = parts[1..].join(" ");
                crate::commands::handle_set_default_model(&model)?;
                Ok(Command::Custom(Event::new("set_default_model", model)))
            },
            
            // ...existing fallback
        }
    } else {
        // ...existing default case
    }
}
```

### 4. Update Configuration Loading in forge_api

Modify the `ForgeLoaderService` in `crates/forge_api/src/loader.rs` to incorporate the `.forge` file:

```rust
// Update imports in crates/forge_api/src/loader.rs
use forge_domain::{ForgeConfig, Workflow};
use std::path::Path;
use std::collections::HashMap;
use serde_json::{Map, Value};

// Add a new enum value for source types
enum WorkflowSource<'a> {
    ExplicitPath(&'a Path),
    ProjectConfig,
    Default,
}

impl<F: Infrastructure> ForgeLoaderService<F> {
    pub async fn load(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        // Determine the workflow source for base configuration
        let base_source = match path {
            Some(path) => WorkflowSource::ExplicitPath(path),
            None if Path::new("forge.yaml").exists() => WorkflowSource::ProjectConfig,
            None => WorkflowSource::Default,
        };

        // Load the base workflow
        let mut workflow = match base_source {
            WorkflowSource::ExplicitPath(path) => self.load_from_path(path).await?,
            WorkflowSource::ProjectConfig => self.load_from_project_config().await?,
            WorkflowSource::Default => create_default_workflow(),
        };
        
        // Apply .forge file settings if it exists
        if Path::new(".forge").exists() {
            self.apply_forge_file_settings(&mut workflow).await?;
        }
        
        Ok(workflow)
    }

    /// Apply settings from the .forge file to the workflow
    async fn apply_forge_file_settings(&self, workflow: &mut Workflow) -> anyhow::Result<()> {
        // Read .forge file
        let forge_config_path = Path::new(".forge");
        let content = String::from_utf8(
            self.0
                .file_read_service()
                .read(forge_config_path)
                .await?
                .to_vec(),
        )?;
        
        // Parse the .forge file
        let forge_config: ForgeConfig = serde_yaml::from_str(&content)?;
        
        // Apply model settings if present
        if let Some(models) = &forge_config.models {
            // Update variables in the workflow
            let variables = workflow.variables.get_or_insert_with(HashMap::new);
            
            // Get or create models map
            let models_value = variables.entry("models".to_string())
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
        models: &HashMap<String, String>
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
                
                for (workflow_role, config_role) in &role_mappings {
                    // If this agent is using a model that matches a role
                    if model.as_str().contains(workflow_role) {
                        if let Some(new_model) = models.get(*config_role) {
                            *model = forge_domain::ModelId::new(new_model.clone());
                            break;
                        }
                    }
                }
                
                // If no role mapping matched but agent has specific function
                if (agent.id.to_string().contains("coding") || 
                   agent.id.to_string().contains("engineer")) && 
                   models.contains_key("coding") {
                    *model = forge_domain::ModelId::new(models.get("coding").unwrap().clone());
                }
            }
            
            // Handle compact model reference if present
            if let Some(compact) = &mut agent.compact {
                // Skip template variables
                if compact.model.as_str().contains("{{") {
                    continue;
                }
                
                if let Some(summarization_model) = models.get("summarization") {
                    compact.model = forge_domain::ModelId::new(summarization_model.clone());
                }
            }
        }
        
        Ok(())
    }
}
```

### 5. Add Unit Tests

Create unit tests in `crates/forge_main/src/commands/config_test.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_forge_file_creation() {
        // Create temp dir for test
        let temp_dir = TempDir::new().unwrap();
        let prev_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();
        
        // Test file creation
        let model_name = "anthropic/claude-3.7-sonnet";
        handle_set_coding_model(model_name).unwrap();
        
        // Verify file was created with correct content
        assert!(Path::new(".forge").exists());
        let content = fs::read_to_string(".forge").unwrap();
        let config: ForgeConfig = serde_yaml::from_str(&content).unwrap();
        
        assert!(config.models.is_some());
        let models = config.models.unwrap();
        assert_eq!(models.get("coding").unwrap(), model_name);
        
        // Clean up
        std::env::set_current_dir(prev_dir).unwrap();
    }
    
    #[test]
    fn test_forge_file_update() {
        // Create temp dir for test
        let temp_dir = TempDir::new().unwrap();
        let prev_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();
        
        // Set initial model
        let initial_model = "anthropic/claude-3.5-haiku";
        handle_set_coding_model(initial_model).unwrap();
        
        // Update model
        let updated_model = "anthropic/claude-3.7-sonnet";
        handle_set_coding_model(updated_model).unwrap();
        
        // Verify update was applied
        let content = fs::read_to_string(".forge").unwrap();
        let config: ForgeConfig = serde_yaml::from_str(&content).unwrap();
        
        assert!(config.models.is_some());
        let models = config.models.unwrap();
        assert_eq!(models.get("coding").unwrap(), updated_model);
        
        // Clean up
        std::env::set_current_dir(prev_dir).unwrap();
    }
}
```

Add integration tests in `crates/forge_api/tests/forge_config_test.rs`:

```rust
#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use anyhow::Result;
    use forge_api::{ForgeAPI, Workflow, API};
    use forge_domain::ForgeConfig;
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_forge_file_precedence() -> Result<()> {
        // Create temp dir for test
        let temp_dir = TempDir::new()?;
        let prev_dir = std::env::current_dir()?;
        std::env::set_current_dir(&temp_dir)?;
        
        // Create forge.yaml with one model setting
        fs::write(
            "forge.yaml",
            r#"
models:
  coding: anthropic/claude-3.5-haiku
            "#,
        )?;
        
        // Create .forge with a different model setting
        let mut forge_config = ForgeConfig::default();
        let mut models = HashMap::new();
        models.insert("coding".to_string(), "anthropic/claude-3.7-sonnet".to_string());
        forge_config.models = Some(models);
        
        let forge_content = serde_yaml::to_string(&forge_config)?;
        fs::write(".forge", forge_content)?;
        
        // Initialize API and load workflow
        let api = ForgeAPI::init(true);
        let workflow = api.load(None).await?;
        
        // Verify that .forge settings take precedence
        let agent = workflow.agents.iter()
            .find(|a| a.id.to_string().contains("coding") || a.id.to_string().contains("engineer"))
            .expect("Couldn't find coding agent");
        
        if let Some(model) = &agent.model {
            assert!(model.as_str().contains("claude-3.7-sonnet"), 
                "Expected model from .forge file, got: {}", model.as_str());
        } else {
            panic!("Agent has no model");
        }
        
        // Clean up
        std::env::set_current_dir(prev_dir)?;
        Ok(())
    }
}
```

### 6. Update Documentation

Create a user guide in `docs/forge-configuration.md`:

```markdown
# Forge Configuration Guide

Forge now supports command-based model configuration through the `.forge` file. This allows you to easily switch between different models for various purposes.

## Commands

### Set Coding Model
Use this command to change the model used for coding tasks:
```
/set-coding-model anthropic/claude-3.7-sonnet
# Output: coding model set to: anthropic/claude-3.7-sonnet
```

### Set Summarization Model
Use this command to change the model used for context summarization:
```
/set-summarization-model google/gemini-pro
# Output: summarization model set to: google/gemini-pro
```

### Set Default Model
Use this command to change the default model:
```
/set-default-model anthropic/claude-3.5-haiku
# Output: default model set to: anthropic/claude-3.5-haiku
```

## Configuration Priority

Forge uses the following priority order for configuration (highest to lowest):
1. Command-line workflow specification (`-w` flag)
2. `.forge` file settings
3. `forge.yaml` settings
4. Default built-in settings

This means that model settings in the `.forge` file will override corresponding settings in `forge.yaml` or the default configuration.

## The `.forge` File

The `.forge` file is automatically created and updated when you use the configuration commands. It's a simple YAML file that contains your model preferences:

```yaml
models:
  coding: anthropic/claude-3.7-sonnet
  default: anthropic/claude-3.5-haiku
  summarization: google/gemini-pro
```

While you can edit this file directly, using the commands is recommended for consistency.
```

## Verification Criteria

The implementation will be successful if:

1. Users can set models using the new commands
2. The `.forge` file is correctly created and updated by the commands
3. Model settings in the `.forge` file take precedence over settings in `forge.yaml` and default configuration
4. The system provides clear feedback when model settings are changed using the CONSOLE
5. All tests pass, demonstrating correct functionality for file creation, updates, and configuration loading

## Future Extensions

This implementation creates a foundation that can be extended to:

1. Add more configuration commands for other settings
2. Create a command to show current configuration
3. Add validation for model names to ensure they follow valid formats
4. Implement a reset command to revert to default settings

These extensions would make the configuration system more robust and user-friendly while maintaining the simplicity of the command-based approach.