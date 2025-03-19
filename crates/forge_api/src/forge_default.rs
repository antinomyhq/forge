//! Default configuration for Forge
//!
//! This module contains the default configuration that is used when no
//! custom configuration is provided.

use forge_domain::Workflow;

// Include the default yaml configuration file as a string
const DEFAULT_YAML: &str = include_str!("../../../forge.default.yaml");

/// Creates the default workflow by parsing the embedded YAML configuration
pub fn create_default_workflow() -> Workflow {
    // Parse the YAML string into a Workflow struct
    let workflow: Workflow = serde_yaml::from_str(DEFAULT_YAML)
        .expect("Failed to parse default forge.yaml configuration");

    workflow
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_workflow_loads_correctly() {
        // This test ensures that the default YAML can be parsed into a Workflow
        let workflow = create_default_workflow();

        // Basic sanity checks
        assert!(
            !workflow.agents.is_empty(),
            "Default workflow should have agents"
        );

        // Check that we have the software-engineer agent
        let has_engineer = workflow
            .agents
            .iter()
            .any(|agent| agent.id.to_string() == "software-engineer");
        assert!(
            has_engineer,
            "Default workflow should have the software-engineer agent"
        );
    }
}
