use async_trait::async_trait;
use forge_domain::{
    ContextMessage, Conversation, Environment, EventData, EventHandle, RequestPayload, Role,
    ToolCallFull, ToolCatalog,
};
use tracing::warn;

const ANALYSIS_PIVOT_REMINDER: &str = "<system-reminder>\nYou have spent several turns exploring without writing or updating the deliverable. Stop analysis now. On your next turn, write a candidate artifact to the final expected path, or make a concrete file-editing step that directly creates the requested deliverable. Only continue exploratory checks after that artifact exists.\n</system-reminder>";

#[derive(Debug, Clone)]
pub struct AnalysisPivotDetector {
    environment: Environment,
}

impl AnalysisPivotDetector {
    const NON_WRITING_TURN_THRESHOLD: usize = 3;

    pub fn new(environment: Environment) -> Self {
        Self { environment }
    }

    fn should_check(&self) -> bool {
        self.environment.background && self.environment.task_timeout_secs.is_some()
    }

    fn reminder_already_sent(conversation: &Conversation) -> bool {
        conversation
            .context
            .as_ref()
            .map(|context| {
                context.messages.iter().any(|message| {
                    message
                        .content()
                        .is_some_and(|content| content.contains(ANALYSIS_PIVOT_REMINDER))
                })
            })
            .unwrap_or(false)
    }

    fn has_writing_tool_call(tool_call: &ToolCallFull) -> bool {
        match tool_call.name.as_str().trim().to_ascii_lowercase().as_str() {
            "write" | "fs_write" | "patch" | "multi_patch" | "remove" | "undo" => true,
            "shell" => match ToolCatalog::try_from(tool_call.clone()) {
                Ok(ToolCatalog::Shell(shell)) => {
                    let command = shell.command.to_ascii_lowercase();
                    command.contains(">") || command.contains("tee ") || command.contains("touch ")
                }
                _ => false,
            },
            _ => false,
        }
    }

    fn recent_non_writing_turns(conversation: &Conversation) -> usize {
        let Some(context) = &conversation.context else {
            return 0;
        };

        let assistant_turns = context.messages.iter().filter_map(|message| match &message.message {
            ContextMessage::Text(text)
                if text.role == Role::Assistant && text.tool_calls.as_ref().is_some() =>
            {
                Some(text)
            }
            _ => None,
        });

        assistant_turns
            .rev()
            .take_while(|text| {
                text.tool_calls
                    .as_ref()
                    .is_some_and(|calls| !calls.iter().any(Self::has_writing_tool_call))
            })
            .count()
    }
}

#[async_trait]
impl EventHandle<EventData<RequestPayload>> for AnalysisPivotDetector {
    async fn handle(
        &self,
        event: &EventData<RequestPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        if !self.should_check() || Self::reminder_already_sent(conversation) {
            return Ok(());
        }

        let non_writing_turns = Self::recent_non_writing_turns(conversation);
        if non_writing_turns < Self::NON_WRITING_TURN_THRESHOLD {
            return Ok(());
        }

        warn!(
            agent_id = %event.agent.id,
            request_count = event.payload.request_count,
            non_writing_turns,
            "Analysis pivot reminder injected after repeated non-writing turns"
        );

        if let Some(context) = conversation.context.take() {
            conversation.context = Some(
                context.add_message(ContextMessage::user(ANALYSIS_PIVOT_REMINDER, None)),
            );
        }

        Ok(())
    }
}

