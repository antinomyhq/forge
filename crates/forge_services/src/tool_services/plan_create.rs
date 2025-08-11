use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use bytes::Bytes;
use chrono::Local;
use forge_app::{PlanCreateOutput, PlanCreateService};

use crate::{FileDirectoryInfra, FileInfoInfra, FileReaderInfra, FileWriterInfra};

/// Creates a new plan file with the specified name, version, and content. Use
/// this tool to create structured project plans, task breakdowns, or
/// implementation strategies that can be tracked and referenced throughout
/// development sessions.
pub struct ForgePlanCreate<F>(Arc<F>);

impl<F> ForgePlanCreate<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self(infra)
    }
}

#[async_trait::async_trait]
impl<F: FileDirectoryInfra + FileInfoInfra + FileReaderInfra + FileWriterInfra + Send + Sync>
    PlanCreateService for ForgePlanCreate<F>
{
    async fn create_plan(
        &self,
        plan_name: String,
        version: String,
        content: String,
    ) -> anyhow::Result<PlanCreateOutput> {
        // Generate the filename with current date
        let current_date = Local::now().format("%Y-%m-%d");
        let filename = format!("{current_date}-{plan_name}-{version}.md");

        // Create the plans directory path (assuming current working directory)
        let plans_dir = Path::new("plans");
        let file_path = plans_dir.join(&filename);

        // Validate the path is reasonable (even though it won't be absolute)
        // Create plans directory if it doesn't exist
        self.0.create_dirs(plans_dir).await.with_context(|| {
            format!("Failed to create plans directory: {}", plans_dir.display())
        })?;

        // Check if the file exists
        let file_exists = self.0.is_file(&file_path).await?;

        // If file exists, return an error - we don't allow overwriting plans
        if file_exists {
            return Err(anyhow::anyhow!(
                "Plan file already exists at {}. Use a different plan name or version to avoid conflicts.",
                file_path.display()
            ));
        }

        // Write the plan file
        self.0
            .write(&file_path, Bytes::from(content), true)
            .await
            .with_context(|| format!("Failed to write plan file: {}", file_path.display()))?;

        Ok(PlanCreateOutput { path: file_path.display().to_string(), before: None })
    }
}
