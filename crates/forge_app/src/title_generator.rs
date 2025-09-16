use std::sync::Arc;

use derive_setters::Setters;
use forge_domain::{
    ChatCompletionMessageFull, Context, ContextMessage, ConversationId, ModelId, ReasoningConfig,
    ResultStreamExt, extract_tag_content,
};
use serde_json::Value;

use crate::agent::AgentService as AS;

/// Service for generating contextually appropriate titles
#[derive(Setters)]
pub struct TitleGenerator<S> {
    /// Shared reference to the agent services used for AI interactions
    services: Arc<S>,
    /// The user prompt to generate a title for
    user_prompt: Value,
    /// The model ID to use for title generation
    model_id: ModelId,
    /// Reasoning configuration for the generator.
    reasoning: Option<ReasoningConfig>,
}

impl<S: AS> TitleGenerator<S> {
    pub fn new(services: Arc<S>, user_prompt: Value, model_id: ModelId) -> Self {
        Self { services, user_prompt, model_id, reasoning: None }
    }

    pub async fn generate(&self) -> anyhow::Result<Option<String>> {
        let template = self
            .services
            .render("{{> forge-system-prompt-title-generation.md }}", &())
            .await?;

        let prompt = format!(
            "<user_prompt>{}</user_prompt>",
            self.user_prompt.as_str().unwrap_or_default()
        );
        let mut ctx = Context::default()
            .temperature(1.0f32)
            .conversation_id(ConversationId::generate())
            .add_message(ContextMessage::system(template))
            .add_message(ContextMessage::user(prompt, Some(self.model_id.clone())));

        // Set the reasoning if configured.
        if let Some(reasoning) = self.reasoning.as_ref() {
            ctx = ctx.reasoning(reasoning.clone());
        }

        let stream = self.services.chat_agent(&self.model_id, ctx).await?;
        let ChatCompletionMessageFull { content, .. } = stream.into_full(false).await?;
        if let Some(extracted) = extract_tag_content(&content, "title") {
            return Ok(Some(extracted.to_string()));
        }
        Ok(None)
    }
}
