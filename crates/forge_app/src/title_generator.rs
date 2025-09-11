use std::sync::Arc;

use forge_domain::{
    ChatCompletionMessageFull, Context, ContextMessage, ModelId, ResultStreamExt,
    extract_tag_content,
};

use crate::agent::AgentService as AS;

/// Service for generating contextually appropriate titles
pub struct TitleGenerator<S> {
    /// Shared reference to the agent services used for AI interactions
    services: Arc<S>,
}

impl<S: AS> TitleGenerator<S> {
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }

    /// Generate the appropriate title for given user prompt.
    pub async fn generate(
        &self,
        user_prompt: &str,
        model_id: &ModelId,
    ) -> anyhow::Result<Option<String>> {
        let system_prompt = include_str!("../../forge_services/src/agents/title-generator.md");
        let ctx = Context::default()
            .add_message(ContextMessage::system(system_prompt))
            .add_message(ContextMessage::user(
                user_prompt.to_string(),
                Some(model_id.clone()),
            ));

        let stream = self.services.chat_agent(model_id, ctx).await?;
        let ChatCompletionMessageFull { content, .. } = stream.into_full(false).await?;
        if let Some(extracted) = extract_tag_content(&content, "title") {
            return Ok(Some(extracted.to_string()));
        }
        Ok(None)
    }
}
