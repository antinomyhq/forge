use std::ops::Deref;
use std::sync::Arc;

use forge_domain::{Agent, *};
use serde_json::json;
use tracing::debug;

use crate::{AttachmentService, TemplateEngine};

/// Service responsible for setting user prompts in the conversation context
#[derive(Clone)]
pub struct UserPromptGenerator<S> {
    services: Arc<S>,
    agent: Agent,
    event: Event,
    current_time: chrono::DateTime<chrono::Local>,
}

impl<S: AttachmentService> UserPromptGenerator<S> {
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
        conversation: Conversation,
    ) -> anyhow::Result<Conversation> {
        let (conversation, content) = self.add_rendered_message(conversation).await?;
        let conversation = self.add_additional_context(conversation).await?;
        let conversation = if let Some(content) = content {
            self.add_attachments(conversation, &content).await?
        } else {
            conversation
        };
        Ok(conversation)
    }

    /// Adds additional context (piped input) as a droppable user message
    async fn add_additional_context(
        &self,
        mut conversation: Conversation,
    ) -> anyhow::Result<Conversation> {
        let mut context = conversation.context.take().unwrap_or_default();

        if let Some(piped_input) = &self.event.additional_context {
            let piped_message = TextMessage {
                role: Role::User,
                content: piped_input.clone(),
                raw_content: None,
                tool_calls: None,
                reasoning_details: None,
                model: Some(self.agent.model.clone()),
                droppable: true, // Piped input is droppable
            };
            context = context.add_message(ContextMessage::Text(piped_message));
        }

        Ok(conversation.context(context))
    }

    /// Renders the user message content and adds it to the conversation
    /// Returns the conversation and the rendered content for attachment parsing
    async fn add_rendered_message(
        &self,
        mut conversation: Conversation,
    ) -> anyhow::Result<(Conversation, Option<String>)> {
        let mut context = conversation.context.take().unwrap_or_default();
        let event_value = self.event.value.clone();
        let template_engine = TemplateEngine::default();

        let content =
            if let Some(user_prompt) = &self.agent.user_prompt
                && self.event.value.is_some()
            {
                let user_input = self
                    .event
                    .value
                    .as_ref()
                    .and_then(|v| v.as_user_prompt().map(|u| u.as_str().to_string()))
                    .unwrap_or_default();
                let mut event_context = EventContext::new(EventContextValue::new(user_input))
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
                        let rendered_prompt = template_engine.render_template(
                            command.template.clone(),
                            &json!({"parameters": command.parameters.join(" ")}),
                        )?;
                        event_context.event(EventContextValue::new(rendered_prompt))
                    }
                    None => event_context,
                };

                // Render the event value into agent's user prompt template.
                Some(template_engine.render_template(
                    Template::new(user_prompt.template.as_str()),
                    &event_context,
                )?)
            } else {
                // Use the raw event value as content if no user_prompt is provided
                event_value
                    .as_ref()
                    .and_then(|v| v.as_user_prompt().map(|p| p.deref().to_owned()))
            };

        if let Some(content) = &content {
            // Create User Message
            let message = TextMessage {
                role: Role::User,
                content: content.clone(),
                raw_content: event_value,
                tool_calls: None,
                reasoning_details: None,
                model: Some(self.agent.model.clone()),
                droppable: false,
            };
            context = context.add_message(ContextMessage::Text(message));
        }

        Ok((conversation.context(context), content))
    }

    /// Parses and adds attachments to the conversation based on the provided
    /// content
    async fn add_attachments(
        &self,
        mut conversation: Conversation,
        content: &str,
    ) -> anyhow::Result<Conversation> {
        let mut context = conversation.context.take().unwrap_or_default();

        // Parse Attachments (do NOT parse piped input for attachments)
        let attachments = self.services.attachments(content).await?;
        context = context.add_attachments(attachments, Some(self.agent.model.clone()));

        Ok(conversation.context(context))
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{AgentId, Context, ContextMessage, ConversationId, ModelId, ProviderId};
    use pretty_assertions::assert_eq;

    use super::*;

    struct MockService;

    #[async_trait::async_trait]
    impl AttachmentService for MockService {
        async fn attachments(&self, _url: &str) -> anyhow::Result<Vec<Attachment>> {
            Ok(Vec::new())
        }
    }

    fn fixture_agent_without_user_prompt() -> Agent {
        Agent::new(
            AgentId::from("test_agent"),
            ProviderId::OPENAI,
            ModelId::from("test-model"),
        )
    }

    fn fixture_conversation() -> Conversation {
        Conversation::new(ConversationId::default()).context(Context::default())
    }

    fn fixture_generator(agent: Agent, event: Event) -> UserPromptGenerator<MockService> {
        UserPromptGenerator::new(Arc::new(MockService), agent, event, chrono::Local::now())
    }

    #[tokio::test]
    async fn test_adds_context_as_droppable_message() {
        let agent = fixture_agent_without_user_prompt();
        let event = Event::new("First Message").additional_context("Second Message");
        let conversation = fixture_conversation();
        let generator = fixture_generator(agent.clone(), event);

        let actual = generator.add_user_prompt(conversation).await.unwrap();

        let messages = actual.context.unwrap().messages;
        assert_eq!(
            messages.len(),
            2,
            "Should have context message and main message"
        );

        // First message should be the context (droppable)
        let task_message = messages.first().unwrap();
        assert_eq!(task_message.content().unwrap(), "First Message");
        assert!(
            !task_message.is_droppable(),
            "Context message should be droppable"
        );

        // Second message should not be droppable
        let context_message = messages.last().unwrap();
        assert_eq!(context_message.content().unwrap(), "Second Message");
        assert!(
            context_message.is_droppable(),
            "Main message should not be droppable"
        );
    }

    #[tokio::test]
    async fn test_context_added_before_main_message() {
        let agent = fixture_agent_without_user_prompt();
        let event = Event::new("First Message").additional_context("Second Message");
        let conversation = fixture_conversation();
        let generator = fixture_generator(agent.clone(), event);

        let actual = generator.add_user_prompt(conversation).await.unwrap();

        let messages = actual.context.unwrap().messages;
        assert_eq!(messages.len(), 2);

        // Verify order: main message first, then additional context
        assert_eq!(messages[0].content().unwrap(), "First Message");
        assert_eq!(messages[1].content().unwrap(), "Second Message");
    }

    #[tokio::test]
    async fn test_no_context_only_main_message() {
        let agent = fixture_agent_without_user_prompt();
        let event = Event::new("Simple task");
        let conversation = fixture_conversation();
        let generator = fixture_generator(agent.clone(), event);

        let actual = generator.add_user_prompt(conversation).await.unwrap();

        let messages = actual.context.unwrap().messages;
        assert_eq!(messages.len(), 1, "Should only have the main message");
        assert_eq!(messages[0].content().unwrap(), "Simple task");
    }

    #[tokio::test]
    async fn test_empty_event_no_message_added() {
        let agent = fixture_agent_without_user_prompt();
        let event = Event::empty();
        let conversation = fixture_conversation();
        let generator = fixture_generator(agent.clone(), event);

        let actual = generator.add_user_prompt(conversation).await.unwrap();

        let messages = actual.context.unwrap().messages;
        assert_eq!(
            messages.len(),
            0,
            "Should not add any message for empty event"
        );
    }

    #[tokio::test]
    async fn test_raw_content_preserved_in_message() {
        let agent = fixture_agent_without_user_prompt();
        let event = Event::new("Task text");
        let conversation = fixture_conversation();
        let generator = fixture_generator(agent.clone(), event);

        let actual = generator.add_user_prompt(conversation).await.unwrap();

        let messages = actual.context.unwrap().messages;
        let message = messages.first().unwrap();

        if let ContextMessage::Text(text_msg) = &**message {
            assert!(
                text_msg.raw_content.is_some(),
                "Raw content should be preserved"
            );
            let raw = text_msg.raw_content.as_ref().unwrap();
            assert_eq!(raw.as_user_prompt().unwrap().as_str(), "Task text");
        } else {
            panic!("Expected TextMessage");
        }
    }
}
