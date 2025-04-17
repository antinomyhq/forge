use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use bytes::Bytes;
use chrono::Local;
use forge_domain::{EnvironmentService, ExecutableTool, NamedTool, ToolDescription, ToolName};
use forge_tool_macros::ToolDescription;
use glob::glob;
use regex::Regex;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::infra::FsMetaService;
use crate::{FsWriteService, Infrastructure};

#[derive(Deserialize, JsonSchema)]
pub struct PlanWriteInput {
    /// Base name for the plan file (sanitized to create a valid filename).
    /// Used in YYYY-MM-DD-{name}-v{version}.md format. Must not be empty.
    pub name: String,

    /// Complete markdown content to write to the plan file.
    /// Should include proper formatting. Must not be empty.
    pub content: String,
}

/// Creates plan files in the "plans/" directory with auto-versioned filenames.
/// Use for project plans, RFCs, or migration strategies. Filenames follow
/// "YYYY-MM-DD-name-v{version}.md", with versions incrementing for same-named
/// plans on the same day. Only writes markdown files to the plans directory and
/// cannot modify existing files. Content should be complete markdown. Plans
/// directory is created if needed. Names are sanitized (spaces to hyphens,
/// special characters removed).
#[derive(ToolDescription)]
pub struct PlanWrite<F>(Arc<F>);

impl<F: Infrastructure> PlanWrite<F> {
    pub fn new(f: Arc<F>) -> Self {
        Self(f)
    }

    /// Get the absolute path to the plans directory
    fn get_plans_dir(&self) -> Result<PathBuf> {
        let env = self.0.environment_service().get_environment();
        let cwd = PathBuf::from(&env.cwd);

        let plans_dir = cwd.join("plans");
        Ok(plans_dir)
    }

    /// Generate a filename based on current date, name and version
    async fn generate_filename(&self, name: &str) -> Result<String> {
        // Get current date in YYYY-MM-DD format
        let date = Local::now().format("%Y-%m-%d").to_string();

        // Sanitize the name: replace spaces with hyphens and remove special characters
        let sanitized_name = self.sanitize_name(name);

        // Find existing versions for this name and date
        let next_version = self.get_next_version(&date, &sanitized_name).await?;

        // Create filename with format: YYYY-MM-DD-name-v{version}.md
        Ok(format!("{}-{}-v{}.md", date, sanitized_name, next_version))
    }

    /// Sanitize a name for use in a filename
    fn sanitize_name(&self, name: &str) -> String {
        // Replace spaces with hyphens
        let name = name.replace(' ', "-");

        // Remove special characters that shouldn't be in filenames
        let re = Regex::new(r"[^a-zA-Z0-9\-_]").unwrap();
        re.replace_all(&name, "").to_string()
    }

    /// Find the next available version number for a given date and name
    async fn get_next_version(&self, date: &str, name: &str) -> Result<u32> {
        let plans_dir = self.get_plans_dir()?;

        // Create the pattern to match: YYYY-MM-DD-name-v*.md
        let pattern = format!("{}/{}-{}-v*.md", plans_dir.display(), date, name);

        // Define a regex to extract version numbers
        let version_regex = Regex::new(&format!(r"{}-{}-v(\d+)\.md", date, name))?;

        // Find the highest existing version
        let mut highest_version = 0;

        for entry_result in glob(&pattern)? {
            // Skip entries that result in an error
            let path = match entry_result {
                Ok(p) => p,
                Err(_) => continue,
            };

            let version = path
                .file_name()
                .and_then(|filename| filename.to_str())
                .and_then(|filename_str| version_regex.captures(filename_str))
                .and_then(|captures| captures.get(1))
                .and_then(|version_match| version_match.as_str().parse::<u32>().ok())
                .unwrap_or(0);

            if version > highest_version {
                highest_version = version;
            }
        }

        // Return the next version (current highest + 1)
        Ok(highest_version + 1)
    }

    /// Ensure the plans directory exists, creating it if needed
    async fn ensure_plans_dir_exists(&self) -> Result<()> {
        let plans_dir = self.get_plans_dir()?;

        // Check if the directory exists
        if !self.0.file_meta_service().exists(&plans_dir).await? {
            // Create the directory if it doesn't exist
            tokio::fs::create_dir_all(&plans_dir)
                .await
                .with_context(|| {
                    format!("Failed to create plans directory: {}", plans_dir.display())
                })?;
        }

        Ok(())
    }
}

impl<F> NamedTool for PlanWrite<F> {
    fn tool_name() -> ToolName {
        ToolName::new("tool_forge_plan_create")
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> ExecutableTool for PlanWrite<F> {
    type Input = PlanWriteInput;

    async fn call(&self, input: Self::Input) -> Result<String> {
        // Validate inputs
        if input.name.is_empty() {
            return Err(anyhow::anyhow!("Plan name cannot be empty"));
        }

        if input.content.is_empty() {
            return Err(anyhow::anyhow!("Plan content cannot be empty"));
        }

        // Ensure the plans directory exists
        self.ensure_plans_dir_exists().await?;

        // Generate the filename with date, name and version
        let filename = self.generate_filename(&input.name).await?;

        // Create the full path
        let plans_dir = self.get_plans_dir()?;
        let file_path = plans_dir.join(&filename);

        // Write the file content
        self.0
            .file_write_service()
            .write(&file_path, Bytes::from(input.content.clone()))
            .await?;

        Ok(format!(
            "Successfully created plan file: plans/{}\nContent length: {} bytes",
            filename,
            input.content.len()
        ))
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::attachment::tests::MockInfrastructure;

    #[tokio::test]
    async fn test_sanitize_name() {
        let infra = Arc::new(MockInfrastructure::new());
        let plan_write = PlanWrite::new(infra);

        let test_cases = vec![
            ("simple", "simple"),
            ("with space", "with-space"),
            ("special@#$chars", "specialchars"),
            ("mixed-case_with_underscores", "mixed-case_with_underscores"),
            ("  leading-trailing  ", "--leading-trailing--"),
        ];

        for (input, expected) in test_cases {
            let result = plan_write.sanitize_name(input);
            assert_eq!(result, expected);
        }
    }

    // Additional tests will be implemented after fixing the mock infrastructure
    // to better support testing file creation
}
