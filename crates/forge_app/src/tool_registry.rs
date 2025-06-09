use std::sync::Arc;

use forge_domain::{Tool, ToolCallFull, ToolDefinition, ToolName, ToolResult};

use crate::Services;

pub struct ToolRegistry<S> {
    #[allow(dead_code)]
    services: Arc<S>,
}
impl<S: Services> ToolRegistry<S> {
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }
    #[allow(dead_code)]
    pub async fn call(&self, _input: ToolCallFull) -> ToolResult {
        unimplemented!()
    }
    #[allow(dead_code)]
    pub async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        unimplemented!()
    }
    #[allow(dead_code)]
    pub async fn find(&self, _: &ToolName) -> anyhow::Result<Option<Arc<Tool>>> {
        unimplemented!()
    }
}
