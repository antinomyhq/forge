use std::sync::Arc;

use forge_domain::*;

use crate::agent::AgentService;

/// Manages plan execution lifecycle within the orchestration loop.
///
/// Tracks plan completion state and coordinates context updates based on
/// active plan progress. Designed to be owned by the orchestrator and called
/// during each iteration.
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
