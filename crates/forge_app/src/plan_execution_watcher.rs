use std::sync::Arc;

use forge_domain::*;

use crate::agent::AgentService;

/// Manages plan execution lifecycle within the orchestration loop.
pub struct PlanExecutionWatcher<'a, S> {
    services: Arc<S>,
    agent: &'a Agent,
    tool_context: &'a ToolCallContext,
    completion_notified: bool,
}

impl<'a, S> PlanExecutionWatcher<'a, S> {
    /// Creates a new plan execution watcher with required dependencies
    pub fn new(services: Arc<S>, agent: &'a Agent, tool_context: &'a ToolCallContext) -> Self {
        Self { services, agent, tool_context, completion_notified: false }
    }
}

impl<'a, S: AgentService> PlanExecutionWatcher<'a, S> {
    /// Adds initial reminder to context if plan_start tool is available
    pub async fn init_context(&self, context: Context) -> anyhow::Result<Context> {
        let plan_start_tool = ToolsDiscriminants::PlanStart.name();

        if self
            .agent
            .tools
            .as_ref()
            .is_some_and(|t| t.contains(&plan_start_tool))
        {
            let reminder = self
                .services
                .render(
                    Template::new("{{> forge-plan-start-reminder.md}}"),
                    &serde_json::json!({ "tool_name": plan_start_tool.as_str() }),
                )
                .await?;

            Ok(context.add_message(ContextMessage::user(reminder, self.agent.model.clone())))
        } else {
            Ok(context)
        }
    }
    /// Updates context based on active plan state. Returns updated context and
    /// whether execution should yield.
    pub async fn update_context(
        &mut self,
        context: Context,
        should_yield: bool,
    ) -> anyhow::Result<(Context, bool)> {
        if !should_yield {
            self.completion_notified = false;
            return Ok((context, should_yield));
        }

        let Ok(Some(active_plan)) = self.tool_context.get_active_plan() else {
            self.completion_notified = false;
            return Ok((context, should_yield));
        };

        // Plan complete with no failures or already notified - allow yield
        if active_plan.is_complete() && (self.completion_notified || active_plan.failed() == 0) {
            return Ok((context, should_yield));
        }

        // Mark completion for plans with failures to give agent one more attempt
        if active_plan.is_complete() && active_plan.failed() > 0 {
            self.completion_notified = true;
        }

        // Render and inject plan notification
        let notification = self.render_notification(&active_plan).await?;
        let updated_context =
            context.add_message(ContextMessage::user(notification, self.agent.model.clone()));

        Ok((updated_context, false))
    }

    async fn render_notification(&self, plan: &ActivePlan) -> anyhow::Result<String> {
        let plan_view = serde_json::json!({
            "path": plan.path,
            "is_complete": plan.is_complete(),
            "next_pending_task": plan.next_pending_task(),
            "tasks_in_progress": plan.tasks_with_status(TaskStatus::InProgress),
            "tasks_failed": plan.tasks_with_status(TaskStatus::Failed),
        });

        self.services
            .render(
                forge_domain::Template::new("{{> forge-plan-notification.md}}"),
                &serde_json::json!({ "plan": plan_view }),
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use pretty_assertions::assert_eq;

    use super::*;

    struct MockService;

    #[async_trait::async_trait]
    impl AgentService for MockService {
        async fn chat_agent(
            &self,
            _id: &ModelId,
            _context: Context,
            _provider_id: Option<ProviderId>,
        ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
            unimplemented!()
        }

        async fn call(
            &self,
            _agent: &Agent,
            _context: &ToolCallContext,
            _call: ToolCallFull,
        ) -> ToolResult {
            unimplemented!()
        }

        async fn render<V: serde::Serialize + Send + Sync>(
            &self,
            _template: Template<V>,
            _data: &V,
        ) -> anyhow::Result<String> {
            Ok("rendered_content".to_string())
        }

        async fn update(&self, _conversation: Conversation) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct TestFixture {
        services: Arc<MockService>,
        agent: Agent,
        tool_context: ToolCallContext,
        context: Context,
    }

    impl TestFixture {
        fn new(with_tool: bool) -> Self {
            let services = Arc::new(MockService);
            let agent = if with_tool {
                Agent::new("test").tools(vec![ToolsDiscriminants::PlanStart.name()])
            } else {
                Agent::new("test")
            };
            let tool_context = ToolCallContext::new(Metrics::default());
            let context = Context::default();
            Self { services, agent, tool_context, context }
        }

        fn watcher(&self) -> PlanExecutionWatcher<MockService> {
            PlanExecutionWatcher::new(self.services.clone(), &self.agent, &self.tool_context)
        }
    }

    fn plan(tasks: Vec<(&str, TaskStatus)>) -> ActivePlan {
        ActivePlan::new(
            PathBuf::from("/test/plan.md"),
            tasks
                .into_iter()
                .enumerate()
                .map(|(i, (desc, status))| Task::new(desc.to_string(), status, i + 1))
                .collect(),
        )
    }

    #[tokio::test]
    async fn test_init_context_adds_reminder_when_tool_available() {
        let fixture = TestFixture::new(true);
        let watcher = fixture.watcher();
        let actual = watcher.init_context(fixture.context.clone()).await.unwrap();

        assert_eq!(actual.messages.len(), fixture.context.messages.len() + 1);
        assert_eq!(
            actual.messages.last().unwrap().content(),
            Some("rendered_content")
        );
    }

    #[tokio::test]
    async fn test_update_context_allows_yield_when_plan_complete_no_failures() {
        let fixture = TestFixture::new(true);
        let mut watcher = fixture.watcher();
        fixture
            .tool_context
            .set_active_plan(plan(vec![
                ("task 1", TaskStatus::Done),
                ("task 2", TaskStatus::Done),
            ]))
            .unwrap();

        let (actual_context, actual_yield) = watcher
            .update_context(fixture.context.clone(), true)
            .await
            .unwrap();

        assert_eq!(
            actual_context.messages.len(),
            fixture.context.messages.len()
        );
        assert_eq!(actual_yield, true);
    }

    #[tokio::test]
    async fn test_update_context_notifies_when_plan_complete_with_failures() {
        let fixture = TestFixture::new(true);
        let mut watcher = fixture.watcher();
        fixture
            .tool_context
            .set_active_plan(plan(vec![
                ("task 1", TaskStatus::Done),
                ("task 2", TaskStatus::Failed),
            ]))
            .unwrap();

        let (actual_context, actual_yield) = watcher
            .update_context(fixture.context.clone(), true)
            .await
            .unwrap();

        assert_eq!(
            actual_context.messages.len(),
            fixture.context.messages.len() + 1
        );
        assert_eq!(actual_yield, false);
        assert_eq!(watcher.completion_notified, true);
    }

    #[tokio::test]
    async fn test_update_context_adds_notification_for_incomplete_plan() {
        let fixture = TestFixture::new(true);
        let mut watcher = fixture.watcher();
        fixture
            .tool_context
            .set_active_plan(plan(vec![
                ("task 1", TaskStatus::Done),
                ("task 2", TaskStatus::Pending),
            ]))
            .unwrap();

        let (actual_context, actual_yield) = watcher
            .update_context(fixture.context.clone(), true)
            .await
            .unwrap();

        assert_eq!(
            actual_context.messages.len(),
            fixture.context.messages.len() + 1
        );
        assert_eq!(
            actual_context.messages.last().unwrap().content(),
            Some("rendered_content")
        );
        assert_eq!(actual_yield, false);
    }
}
