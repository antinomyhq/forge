use std::ops::Deref;
use std::sync::Arc;

use forge_domain::*;
use serde_json::json;
use tracing::debug;

use crate::{AttachmentService, TemplateService};

/// Service responsible for setting user prompts in the conversation context
#[derive(Clone)]
pub struct UserPromptGenerator<S> {
    services: Arc<S>,
    agent: Agent,
    event: Event,
    current_time: chrono::DateTime<chrono::Local>,
}

impl<S: TemplateService + AttachmentService> UserPromptGenerator<S> {
    /// Creates a new UserPromptService
    pub fn new(
        service: Arc<S>,
        agent: Agent,
        event: Event,
        current_time: chrono::DateTime<chrono::Local>,
    ) -> Self {
        Self { services: service, agent, event, current_time }
    }

    /// Sets the user prompt in the context based on agent configuration and
    /// event data
    pub async fn add_user_prompt(
        &self,
        mut conversation: Conversation,
    ) -> anyhow::Result<Conversation> {
        let mut context = conversation.context.take().unwrap_or_default();
        let event_value = self.event.value.clone();

        let content = if let Some(user_prompt) = &self.agent.user_prompt
            && self.event.value.is_some()
        {
            let user_input = self
                .event
                .value
                .as_ref()
                .and_then(|v| v.as_user_prompt().map(|u| u.as_str().to_string()))
                .unwrap_or_default();
            let mut event_context =
                EventContext::new(EventContextValue::new(self.event.name.clone(), user_input))
                    .current_date(self.current_time.format("%Y-%m-%d").to_string());

            // Check if context already contains user messages to determine if it's feedback
            let has_user_messages = context.messages.iter().any(|msg| msg.has_role(Role::User));

            if has_user_messages {
                event_context = event_context.into_feedback();
            } else {
                event_context = event_context.into_task();
            }

            debug!(event_context = ?event_context, "Event context");

            // Render the command first.
            let event_context = match self.event.value.as_ref().and_then(|v| v.as_command()) {
                Some(command) => {
                    let rendered_prompt = self
                        .services
                        .render_template(
                            command.template.clone(),
                            &json!({"parameters": command.parameters.join(" ")}),
                        )
                        .await?;
                    event_context.event(EventContextValue::new(
                        self.event.name.clone(),
                        rendered_prompt,
                    ))
                }
                None => event_context,
            };

            // Render the event value into agent's user prompt template.
            Some(
                self.services
                    .render_template(Template::new(user_prompt.template.as_str()), &event_context)
                    .await?,
            )
        } else {
            // Use the raw event value as content if no user_prompt is provided
            event_value
                .as_ref()
                .and_then(|v| v.as_user_prompt().map(|p| p.deref().to_owned()))
        };

        if let Some(content) = content {
            // Parse Attachments
            let attachments = self.services.attachments(content.as_str()).await?;

            // Create User Message
            let message = TextMessage {
                role: Role::User,
                content,
                raw_content: event_value,
                tool_calls: None,
                reasoning_details: None,
                model: self.agent.model.clone(),
                droppable: false,
            };
            context = context
                .add_message(ContextMessage::Text(message))
                .add_attachments(attachments, self.agent.model.clone());
        }

        Ok(conversation.context(context))
    }
}
