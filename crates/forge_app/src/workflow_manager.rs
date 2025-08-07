use std::path::Path;
use std::sync::Arc;

use forge_domain::{Policy, Workflow};
use merge::Merge;
use tokio::sync::Mutex;

use crate::{AgentLoaderService, PolicyLoaderService, WorkflowService};

pub struct WorkflowManager<S> {
    service: Arc<S>,
    extended_policies: Arc<Mutex<Vec<Policy>>>,
}

impl<S: WorkflowService + AgentLoaderService + PolicyLoaderService + Sized> WorkflowManager<S> {
    pub fn new(service: Arc<S>) -> WorkflowManager<S> {
        Self { 
            service,
            extended_policies: Arc::new(Mutex::new(Vec::new())),
        }
    }
    async fn extend_agents(&self, mut workflow: Workflow) -> Workflow {
        let agents = self.service.load_agents().await.unwrap_or_default();
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
        workflow
    }

    async fn extend_policies(&self, mut workflow: Workflow) -> Workflow {
        let loaded_policies = self.service.load_policies().await.unwrap_or_default();

        {
            let mut extended = self.extended_policies.lock().await;
            extended.extend(loaded_policies.policies.iter().cloned());
        }

        // If there are loaded policies, merge them with existing workflow policies
        if !loaded_policies.policies.is_empty() {
            if let Some(existing_policies) = workflow.policies.as_mut() {
                // Merge the loaded policies into existing ones
                for policy in loaded_policies.policies {
                    *existing_policies = existing_policies.clone().add_policy(policy);
                }
            } else {
                // No existing policies, just use the loaded ones
                workflow.policies = Some(loaded_policies);
            }
        }

        workflow
    }
    pub async fn read_workflow(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        let mut workflow = self.service.read_workflow(path).await?;
        workflow = self.extend_agents(workflow).await;
        workflow = self.extend_policies(workflow).await;
        Ok(workflow)
    }
    pub async fn read_merged(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        let mut workflow = self.service.read_merged(path).await?;
        workflow = self.extend_agents(workflow).await;
        workflow = self.extend_policies(workflow).await;
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

        // Remove policies that were extended from external sources
        if let Some(ref mut policies) = workflow_to_write.policies {
            let mut extended_policies = self.extended_policies.lock().await;
            // Remove policies that match those from extended_policies
            for extended_policy in extended_policies.iter() {
                policies.policies.remove(extended_policy);
            }
            extended_policies.clear();
            
            // If no policies remain, set to None
            if policies.policies.is_empty() {
                workflow_to_write.policies = None;
            }
        }

        self.service.write_workflow(path, &workflow_to_write).await
    }
}
