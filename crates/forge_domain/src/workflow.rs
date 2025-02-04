use std::collections::HashMap;

use derive_more::derive::{Display, From};

use crate::AgentId;

#[derive(Debug, Display, Eq, PartialEq, Hash, Clone)]
pub struct WorkflowId(String);

pub struct Workflow {
    pub id: WorkflowId,
    pub description: String,
    pub handovers: HashMap<FlowId, Vec<FlowId>>,
}

impl Workflow {
    /// Returns flows that have no predecessors
    pub fn head_flow(&self) -> Vec<FlowId> {
        todo!()
    }
}

#[derive(Debug, Display, Eq, PartialEq, From, Hash)]
pub enum FlowId {
    Agent(AgentId),
    Workflow(WorkflowId),
}

pub struct Handover {
    pub from: FlowId,
    pub to: FlowId,
}
