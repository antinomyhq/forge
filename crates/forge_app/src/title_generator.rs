use std::sync::Arc;

use forge_domain::{
    ChatCompletionMessageFull, Context, ContextMessage, ModelId, ResultStreamExt, Role,
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

    pub async fn generate(
        &self,
        context: &Context,
        model_id: &ModelId,
    ) -> anyhow::Result<Option<String>> {
        let first_user_message = context
            .messages
            .iter()
            .find(|message| message.has_role(Role::User));
        if let Some(ContextMessage::Text(text_msg)) = first_user_message
            && let Ok(conversation_title) = self
                .generate_internal(text_msg.content.as_str(), model_id)
                .await
        {
            return Ok(conversation_title);
        }
        Ok(None)
    }

    /// Generate the appropriate title for given user prompt.
    async fn generate_internal(
        &self,
        user_prompt: &str,
        model_id: &ModelId,
    ) -> anyhow::Result<Option<String>> {
        let template = self
            .services
            .render("{{> forge-system-prompt-title-generation.md }}", &())
            .await?;
        let ctx = Context::default()
            .add_message(ContextMessage::system(template))
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
