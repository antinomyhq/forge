use std::ops::Deref;
use std::sync::Arc;

use forge_domain::{Agent, *};
use forge_template::Element;
use serde_json::json;
use tracing::{debug, warn};

use crate::{AttachmentService, SkillFetchService, TemplateEngine};

/// Service responsible for setting user prompts in the conversation context
#[derive(Clone)]
pub struct UserPromptGenerator<S> {
    services: Arc<S>,
    agent: Agent,
    event: Event,
    current_time: chrono::DateTime<chrono::Local>,
}

impl<S: AttachmentService + SkillFetchService> UserPromptGenerator<S> {
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

        // Extract original user input for skill recommendation
        let user_input = self
            .event
            .value
            .as_ref()
            .and_then(|v| v.as_user_prompt().map(|u| u.as_str()));

        let conversation = self
            .add_recommended_skills(conversation, user_input)
            .await?;
        let conversation = if let Some(content) = content {
            self.add_attachments(conversation, &content).await?
        } else {
            conversation
        };
        Ok(conversation)
    }

    /// Adds recommended skills as a droppable user message
    async fn add_recommended_skills(
        &self,
        mut conversation: Conversation,
        user_prompt: Option<&str>,
    ) -> anyhow::Result<Conversation> {
        let Some(user_prompt) = user_prompt else {
            return Ok(conversation);
        };

        // Call skill recommendation service
        let selected_skills = match self
            .services
            .recommend_skills(user_prompt.to_string())
            .await
        {
            Ok(skills) => skills,
            Err(e) => {
                warn!(error = %e, "Failed to recommend skills, continuing without recommendations");
                return Ok(conversation);
            }
        };

        if selected_skills.is_empty() {
            return Ok(conversation);
        }

        // Format the selected skills as a message
        let skills_content = format!(
            "Here are the recommended skills. Use them only if relevant to the user's query. Do not mention these recommendations to the user.\n{}",
            Element::new("recommended_skills").append(selected_skills.iter().map(Element::from))
        );

        let ctx =
            conversation
                .context
                .take()
                .unwrap_or_default()
                .add_message(ContextMessage::Text(
                    TextMessage::new(Role::User, skills_content)
                        .model(self.agent.model.clone())
                        .droppable(true),
                ));

        Ok(conversation.context(ctx))
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

    #[derive(Default)]
    struct MockService {
        recommended_skills: Vec<SelectedSkill>,
    }

    #[async_trait::async_trait]
    impl AttachmentService for MockService {
        async fn attachments(&self, _url: &str) -> anyhow::Result<Vec<Attachment>> {
            Ok(Vec::new())
        }
    }

    #[async_trait::async_trait]
    impl SkillFetchService for MockService {
        async fn fetch_skill(&self, _skill_name: String) -> anyhow::Result<forge_domain::Skill> {
            unimplemented!()
        }

        async fn list_skills(&self) -> anyhow::Result<Vec<forge_domain::Skill>> {
            Ok(vec![])
        }

        async fn recommend_skills(
            &self,
            _use_case: String,
        ) -> anyhow::Result<Vec<forge_domain::SelectedSkill>> {
            Ok(self.recommended_skills.clone())
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
        UserPromptGenerator::new(
            Arc::new(MockService::default()),
            agent,
            event,
            chrono::Local::now(),
        )
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

    #[tokio::test]
    async fn test_recommended_skills_added_as_droppable_message() {
        // Fixture
        let agent = fixture_agent_without_user_prompt();
        let event = Event::new("Help me with PDF files");
        let conversation = fixture_conversation();
        let mock_service = MockService {
            recommended_skills: vec![
                SelectedSkill::new("pdf-handler", 0.95, 1),
                SelectedSkill::new("file-converter", 0.80, 2),
            ],
        };
        let generator =
            UserPromptGenerator::new(Arc::new(mock_service), agent, event, chrono::Local::now());

        // Act
        let actual = generator.add_user_prompt(conversation).await.unwrap();

        // Assert
        let messages = actual.context.unwrap().messages;
        assert_eq!(
            messages.len(),
            2,
            "Should have user message and skills message"
        );

        // First message is the user prompt
        assert_eq!(messages[0].content().unwrap(), "Help me with PDF files");
        assert!(!messages[0].is_droppable());

        // Second message is the recommended skills (droppable)
        let skills_message = &messages[1];
        assert!(
            skills_message.is_droppable(),
            "Skills message should be droppable"
        );
        assert!(
            skills_message
                .content()
                .unwrap()
                .contains("recommended_skills"),
            "Should contain recommended_skills element"
        );
        assert!(
            skills_message.content().unwrap().contains("pdf-handler"),
            "Should contain pdf-handler skill"
        );
    }
}
