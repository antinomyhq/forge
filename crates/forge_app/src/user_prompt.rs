use std::sync::Arc;

use forge_domain::*;
use tracing::debug;

use crate::agent::AgentService;

/// Service responsible for setting user prompts in the conversation context
#[derive(Clone)]
pub struct UserPromptBuilder<S> {
    services: Arc<S>,
    agent: Agent,
    event: Event,
    current_time: chrono::DateTime<chrono::Local>,
}

impl<S> UserPromptBuilder<S> {
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
    pub async fn set_user_prompt(&self, mut context: Context) -> anyhow::Result<Context>
    where
        S: AgentService,
    {
        let content = if let Some(user_prompt) = &self.agent.user_prompt
            && self.event.value.is_some()
        {
            let mut event_context = EventContext::new(self.event.clone())
                .current_date(self.current_time.format("%Y-%m-%d").to_string());

            // Check if context already contains user messages to determine if it's feedback
            let has_user_messages = context.messages.iter().any(|msg| msg.has_role(Role::User));

            if has_user_messages {
                event_context = event_context.into_feedback();
            } else {
                event_context = event_context.into_task();
            }

            debug!(event_context = ?event_context, "Event context");
            Some(
                self.services
                    .render(user_prompt.template.as_str(), &event_context)
                    .await?,
            )
        } else {
            // Use the raw event value as content if no user_prompt is provided
            self.event.value.as_ref().map(|v| v.to_string())
        };

        if let Some(content) = content {
            context = context.add_message(ContextMessage::user(content, self.agent.model.clone()));
        }

        Ok(context)
    }
}
