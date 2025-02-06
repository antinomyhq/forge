#![allow(dead_code)]

use crate::variables::Variables;
use crate::{Agent, AgentId, FlowId, Schema, ToolDefinition, ToolName, Workflow};

#[async_trait::async_trait]
pub trait AgentExecutor {
    async fn execute(&self, agent: &Agent) -> anyhow::Result<Variables>;
}

pub struct Arena {
    pub agents: Vec<Agent>,
    pub workflows: Vec<Workflow>,
    pub tools: Vec<SmartTool<Variables>>,
}

impl Arena {
    pub fn find_agent(&self, id: &AgentId) -> Option<&Agent> {
        self.agents.iter().find(|a| a.id == *id)
    }
}

#[derive(Debug, Clone)]
pub struct SmartTool<S> {
    pub name: ToolName,
    pub description: String,
    pub run: FlowId,
    pub input: Schema<S>,
}

impl<S> SmartTool<S> {
    pub fn to_tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name.clone(),
            description: self.description.clone(),
            input_schema: self.input.schema.clone(),
            output_schema: None,
        }
    }
}
