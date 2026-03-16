use std::sync::Arc;

use async_trait::async_trait;
use forge_domain::{
    ContextMessage, Conversation, EventData, EventHandle, Role, SelectedSkill, StartPayload,
    TextMessage,
};
use forge_template::Element;
use tracing::warn;

use crate::WorkspaceService;

/// Hook handler that injects skill recommendations as a droppable user message
/// at the start of each conversation turn.
///
/// When the `Start` lifecycle event fires the handler:
/// 1. Extracts the raw user query from the most recent user message in the
///    conversation context.
/// 2. Calls [`WorkspaceService::recommend_skills`] which sends the query and
///    all available skills to the remote ranking service and returns only the
///    relevant skills with their relevance scores.
/// 3. Injects a droppable `User` message listing the recommended skills wrapped
///    in `<recommended_skills>` XML so the LLM can decide which to invoke.
///
/// The injected message is marked droppable so it is automatically removed
/// during context compaction.
#[derive(Clone)]
pub struct SkillRecommendationHandler<S> {
    services: Arc<S>,
}

impl<S> SkillRecommendationHandler<S> {
    /// Creates a new skill recommendation handler.
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }

    /// Builds the recommendation message content from a list of selected
    /// skills.
    fn build_message(skills: &[SelectedSkill]) -> String {
        format!(
            "Here are the recommended skills for the user task. Use them only if relevant to the \
             user's query. Do not mention these recommendations to the user.\n{}",
            Element::new("recommended_skills").append(skills.iter().map(Element::from))
        )
    }
}

#[async_trait]
impl<S: WorkspaceService> EventHandle<EventData<StartPayload>>
    for SkillRecommendationHandler<S>
{
    async fn handle(
        &self,
        event: &EventData<StartPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        // Extract the user query from the most-recent user message.
        // Prefer the raw_content (original event value before template rendering);
        // fall back to the rendered content string when raw_content is absent.
        let user_query = conversation
            .context
            .as_ref()
            .and_then(|c| c.messages.iter().rev().find(|m| m.has_role(Role::User)))
            .and_then(|entry| {
                entry
                    .message
                    .as_value()
                    .and_then(|v| v.as_user_prompt())
                    .map(|p| p.as_str().to_owned())
                    .or_else(|| entry.message.content().map(str::to_owned))
            });

        let Some(user_query) = user_query else {
            return Ok(());
        };

        // Call the remote ranking service to get relevant skills for this query.
        let selected = match self.services.recommend_skills(user_query.clone()).await {
            Ok(s) => s,
            Err(e) => {
                warn!(
                    agent_id = %event.agent.id,
                    error = %e,
                    "Failed to recommend skills, skipping"
                );
                return Ok(());
            }
        };

        if selected.is_empty() {
            return Ok(());
        }

        // Inject as a droppable user message so it can be removed during compaction.
        let message = TextMessage::new(Role::User, Self::build_message(&selected))
            .model(event.agent.model.clone())
            .droppable(true);

        let ctx = conversation
            .context
            .take()
            .unwrap_or_default()
            .add_message(ContextMessage::Text(message));
        conversation.context = Some(ctx);

        tracing::debug!(
            agent_id = %event.agent.id,
            user_query = %user_query,
            skills = ?selected.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            "Injected skill recommendations"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{
        AgentId, Context, ContextMessage, Conversation, ConversationId, ModelId, ProviderId, Role,
        SelectedSkill,
    };
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::WorkspaceService;

    // ---------------------------------------------------------------------------
    // Fixtures
    // ---------------------------------------------------------------------------

    fn fixture_agent() -> forge_domain::Agent {
        forge_domain::Agent::new(AgentId::from("test"), ProviderId::OPENAI, ModelId::from("gpt-4"))
    }

    fn fixture_start_event() -> EventData<StartPayload> {
        EventData::new(fixture_agent(), ModelId::from("gpt-4"), StartPayload)
    }

    fn fixture_conversation_with_user_msg(msg: &str) -> Conversation {
        Conversation::new(ConversationId::generate())
            .context(Context::default().add_message(ContextMessage::user(msg, None)))
    }

    // ---------------------------------------------------------------------------
    // Mock service
    // ---------------------------------------------------------------------------

    struct MockWorkspaceService {
        recommended: Vec<SelectedSkill>,
    }

    #[async_trait::async_trait]
    impl WorkspaceService for MockWorkspaceService {
        async fn sync_workspace(
            &self,
            _path: std::path::PathBuf,
            _batch_size: usize,
        ) -> anyhow::Result<forge_stream::MpscStream<anyhow::Result<forge_domain::SyncProgress>>>
        {
            unimplemented!()
        }

        async fn query_workspace(
            &self,
            _path: std::path::PathBuf,
            _params: SearchParams<'_>,
        ) -> anyhow::Result<Vec<forge_domain::Node>> {
            unimplemented!()
        }

        async fn list_workspaces(&self) -> anyhow::Result<Vec<WorkspaceInfo>> {
            unimplemented!()
        }

        async fn get_workspace_info(
            &self,
            _path: std::path::PathBuf,
        ) -> anyhow::Result<Option<WorkspaceInfo>> {
            unimplemented!()
        }

        async fn delete_workspace(
            &self,
            _workspace_id: &WorkspaceId,
        ) -> anyhow::Result<()> {
            unimplemented!()
        }

        async fn delete_workspaces(
            &self,
            _workspace_ids: &[WorkspaceId],
        ) -> anyhow::Result<()> {
            unimplemented!()
        }

        async fn is_indexed(&self, _path: &std::path::Path) -> anyhow::Result<bool> {
            unimplemented!()
        }

        async fn get_workspace_status(
            &self,
            _path: std::path::PathBuf,
        ) -> anyhow::Result<Vec<forge_domain::FileStatus>> {
            unimplemented!()
        }

        async fn is_authenticated(&self) -> anyhow::Result<bool> {
            unimplemented!()
        }

        async fn init_auth_credentials(&self) -> anyhow::Result<WorkspaceAuth> {
            unimplemented!()
        }

        async fn init_workspace(
            &self,
            _path: std::path::PathBuf,
        ) -> anyhow::Result<WorkspaceId> {
            unimplemented!()
        }

        async fn recommend_skills(
            &self,
            _use_case: String,
        ) -> anyhow::Result<Vec<SelectedSkill>> {
            Ok(self.recommended.clone())
        }
    }

    fn fixture_handler(
        recommended: Vec<SelectedSkill>,
    ) -> SkillRecommendationHandler<MockWorkspaceService> {
        SkillRecommendationHandler::new(Arc::new(MockWorkspaceService { recommended }))
    }

    // ---------------------------------------------------------------------------
    // Tests
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_injects_skills_as_droppable_message() {
        // Fixture
        let recommended = vec![
            SelectedSkill::new("pdf", 0.95, 1),
            SelectedSkill::new("excel", 0.80, 2),
        ];
        let handler = fixture_handler(recommended);
        let mut conversation = fixture_conversation_with_user_msg("Help me with a PDF");
        let event = fixture_start_event();

        // Act
        handler.handle(&event, &mut conversation).await.unwrap();

        // Assert
        let messages = conversation.context.unwrap().messages;
        assert_eq!(messages.len(), 2, "Should have original message + recommendation");

        let recommendation = &messages[1];
        assert!(recommendation.is_droppable(), "Recommendation must be droppable");
        let content = recommendation.content().unwrap();
        assert!(content.contains("recommended_skills"), "Should contain XML tag");
        assert!(content.contains("pdf"), "Should mention pdf skill");
        assert!(content.contains("excel"), "Should mention excel skill");
    }

    #[tokio::test]
    async fn test_message_marked_as_user_role() {
        // Fixture
        let recommended = vec![SelectedSkill::new("pdf", 0.95, 1)];
        let handler = fixture_handler(recommended);
        let mut conversation = fixture_conversation_with_user_msg("summarize this PDF");
        let event = fixture_start_event();

        // Act
        handler.handle(&event, &mut conversation).await.unwrap();

        // Assert
        let messages = conversation.context.unwrap().messages;
        let last = messages.last().unwrap();
        assert!(
            last.has_role(Role::User),
            "Recommendation must have User role"
        );
    }

    #[tokio::test]
    async fn test_no_skills_skips_injection() {
        // Fixture – recommend_skills returns empty
        let handler = fixture_handler(vec![]);
        let mut conversation = fixture_conversation_with_user_msg("do something");
        let event = fixture_start_event();

        // Act
        handler.handle(&event, &mut conversation).await.unwrap();

        // Assert – no extra message added
        let messages = conversation.context.unwrap().messages;
        assert_eq!(messages.len(), 1, "No recommendation added when no skills returned");
    }

    #[tokio::test]
    async fn test_no_user_message_skips_injection() {
        // Fixture
        let recommended = vec![SelectedSkill::new("pdf", 0.95, 1)];
        let handler = fixture_handler(recommended);
        // Conversation with only a system message (no user message)
        let mut conversation = Conversation::new(ConversationId::generate())
            .context(Context::default().add_message(ContextMessage::system("system prompt")));
        let event = fixture_start_event();

        // Act
        handler.handle(&event, &mut conversation).await.unwrap();

        // Assert – context unchanged
        let messages = conversation.context.unwrap().messages;
        assert_eq!(messages.len(), 1, "Should not inject when no user message exists");
    }

    #[tokio::test]
    async fn test_skills_appear_in_message_content() {
        // Fixture
        let recommended = vec![
            SelectedSkill::new("debug-cli", 0.90, 1),
            SelectedSkill::new("create-skill", 0.70, 2),
        ];
        let handler = fixture_handler(recommended);
        let mut conversation = fixture_conversation_with_user_msg("help me debug");
        let event = fixture_start_event();

        // Act
        handler.handle(&event, &mut conversation).await.unwrap();

        // Assert
        let messages = conversation.context.unwrap().messages;
        let content = messages[1].content().unwrap();
        assert!(content.contains("debug-cli"));
        assert!(content.contains("create-skill"));
        assert!(
            content.contains("Do not mention these recommendations to the user"),
            "Should include the guidance prefix"
        );
    }
}
