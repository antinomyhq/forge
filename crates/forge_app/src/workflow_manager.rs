use std::path::Path;
use std::sync::Arc;

use forge_domain::Workflow;

use crate::{AgentLoaderService, WorkflowService};

pub struct WorkflowManager<S> {
    service: Arc<S>,
}

impl<S: WorkflowService + AgentLoaderService + Sized> WorkflowManager<S> {
    pub fn new(service: Arc<S>) -> WorkflowManager<S> {
        Self { service }
    }
    pub async fn read_workflow(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        let workflow = self.service.read_workflow(path).await?;
        Ok(workflow)
    }
    pub async fn read_merged(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        let workflow = self.service.read_merged(path).await?;
        Ok(workflow)
    }
    pub async fn write_workflow(
        &self,
        path: Option<&Path>,
        workflow: &Workflow,
    ) -> anyhow::Result<()> {
        self.service.write_workflow(path, workflow).await
    }
}
