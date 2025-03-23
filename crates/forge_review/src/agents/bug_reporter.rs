use std::sync::Arc;

use crate::domain::PullRequest;

use super::SummaryAgent;

pub struct BugReporterAgent {
    review: Arc<PullRequest>,
}

impl BugReporterAgent {
    pub fn new(review: Arc<PullRequest>) -> Self {
        Self { review }
    }
}

#[async_trait::async_trait]
impl SummaryAgent for BugReporterAgent {
    async fn summarize(&self) -> anyhow::Result<String> {
        todo!()
    }
}