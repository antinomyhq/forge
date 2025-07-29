use crate::{AgentLoaderService, WorkflowService};
use std::sync::Arc;

pub struct WorkflowCoordinator<S> {
    service: Arc<S>,
}

impl<S: WorkflowService + AgentLoaderService + Sized> WorkflowCoordinator<S> {
    pub fn new(service: Arc<S>) -> WorkflowCoordinator<S> {
        Self { service }
    }
    
}
