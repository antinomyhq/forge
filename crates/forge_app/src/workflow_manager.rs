use std::path::Path;
use std::sync::Arc;

use forge_domain::Workflow;
use merge::Merge;

use crate::{AgentLoaderService, WorkflowService};

pub struct WorkflowManager<S> {
    service: Arc<S>,
}

impl<S: WorkflowService + AgentLoaderService + Sized> WorkflowManager<S> {
    pub fn new(service: Arc<S>) -> WorkflowManager<S> {
        Self { service }
    }
    async fn extend_agents(&self, mut workflow: Workflow) -> anyhow::Result<Workflow> {
        let agents = self.service.load_agents().await?;
        for agent_def in agents {
            // Check if an agent with this ID already exists in the workflow
            if let Some(existing_agent) = workflow.agents.iter_mut().find(|a| a.id == agent_def.id)
            {
                // Merge the loaded agent into the existing one
                existing_agent.merge(agent_def);
            } else {
                // Add the new agent to the workflow
                workflow.agents.push(agent_def);
            }
        }
        Ok(workflow)
    }
    pub async fn read_workflow(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        let mut workflow = self.service.read_workflow(path).await?;
        workflow = self.extend_agents(workflow).await?;
        Ok(workflow)
    }
    pub async fn read_merged(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        let mut workflow = self.service.read_merged(path).await?;
        workflow = self.extend_agents(workflow).await?;
        Ok(workflow)
    }
    pub async fn write_workflow(
        &self,
        path: Option<&Path>,
        workflow: &Workflow,
    ) -> anyhow::Result<()> {
        // Create a copy of the workflow and remove agents that were loaded from
        // external sources
        let mut workflow_to_write = workflow.clone();
        let loaded_agents = self.service.load_agents().await.unwrap_or_default();

        // Remove agents that were loaded externally (keep only original workflow
        // agents)
        workflow_to_write.agents.retain(|agent| {
            !loaded_agents
                .iter()
                .any(|loaded_agent| loaded_agent.id == agent.id)
        });

        self.service.write_workflow(path, &workflow_to_write).await
    }
}
