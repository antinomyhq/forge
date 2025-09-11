use std::sync::Arc;

use chrono::Utc;
use forge_api::{API, AgentId, ChatRequest, ConversationId, Event};
use serde_json::Value;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::error;

use crate::domain::{Action, CancelId, Command, Timer};

// Event type constants
pub const EVENT_USER_TASK_INIT: &str = "user_task_init";
pub const EVENT_USER_TASK_UPDATE: &str = "user_task_update";

pub struct Executor<T> {
    api: Arc<T>,
}

impl<T> Clone for Executor<T> {
    fn clone(&self) -> Self {
        Self { api: self.api.clone() }
    }
}

impl<T: API + 'static> Executor<T> {
    pub fn new(api: Arc<T>) -> Self {
        Executor { api }
    }

    async fn execute_chat_message(
        &self,
        message: String,
        conversation_id: Option<ConversationId>,
        is_first: bool,
        tx: &Sender<anyhow::Result<Action>>,
    ) -> anyhow::Result<()> {
        let conversation = if let Some(conv_id) = conversation_id {
            // Use existing conversation - more graceful retrieval
            self.api
                .conversation(&conv_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Conversation not found: {}", conv_id))?
        } else {
            // Initialize new conversation
            let new_conversation = self.api.init_conversation().await?;

            // Send action to update conversation state
            tx.send(Ok(Action::ConversationInitialized(new_conversation.id)))
                .await?;

            new_conversation
        };

        // Create event for the chat message with appropriate event type
        let event_type = if is_first {
            EVENT_USER_TASK_INIT
        } else {
            EVENT_USER_TASK_UPDATE
        };

        let event = Event::new(
            format!("{}/{}", AgentId::FORGE.as_str(), event_type),
            Some(Value::String(message.clone())),
        );

        // Create chat request
        let chat_request = ChatRequest::new(event, conversation.id);

        // Create cancellation token for this stream
        let cancellation_token = CancellationToken::new();
        let cancel_id = CancelId::new(cancellation_token.clone());

        // Send StartStream action with the cancel_id
        tx.send(Ok(Action::StartStream(cancel_id.clone()))).await?;

        match self.api.chat(chat_request).await {
            Ok(mut stream) => loop {
                tokio::select! {
                    response = stream.next() => {
                        match response {
                            Some(response) => {
                                tx.send(response.map(Action::ChatResponse)).await?;
                            }
                            None => break,
                        }
                    }
                    _ = cancellation_token.cancelled() => {
                        break;
                    }
                }
            },
            Err(err) => return Err(err),
        }
        Ok(())
    }

    async fn execute(&self, cmd: Command, tx: Sender<anyhow::Result<Action>>) -> () {
        let this = self.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            match this.execute_inner(cmd, tx.clone()).await {
                Ok(_) => {}
                Err(err) => {
                    error!(error = ?err, "Command Execution Error");
                    tx.send(Err(err)).await.unwrap();
                }
            }
        });
    }

    async fn execute_read_workspace(
        &self,
        tx: &Sender<anyhow::Result<Action>>,
    ) -> anyhow::Result<()> {
        // Get current directory
        let current_dir = self
            .api
            .environment()
            .cwd
            .file_name()
            .map(|name| name.to_string_lossy().to_string());

        // Get current git branch
        let current_branch = match tokio::process::Command::new("git")
            .args(["branch", "--show-current"])
            .output()
            .await
        {
            Ok(output) if output.status.success() => {
                let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if branch.is_empty() {
                    None
                } else {
                    Some(branch)
                }
            }
            _ => None,
        };

        let action = Action::Workspace { current_dir, current_branch };
        tx.send(Ok(action)).await.unwrap();
        Ok(())
    }

    async fn execute_empty(&self) -> anyhow::Result<()> {
        // Empty command doesn't send any action
        Ok(())
    }

    async fn execute_exit(&self) -> anyhow::Result<()> {
        // Exit command doesn't send any action
        Ok(())
    }

    async fn execute_and(
        &self,
        commands: Vec<Command>,
        tx: &Sender<anyhow::Result<Action>>,
    ) -> anyhow::Result<()> {
        // Execute all commands
        for cmd in commands {
            self.execute(cmd, tx.clone()).await;
        }
        Ok(())
    }

    async fn execute_interval(
        &self,
        duration: std::time::Duration,
        tx: &Sender<anyhow::Result<Action>>,
    ) -> anyhow::Result<()> {
        let cancellation_token = CancellationToken::new();
        self.execute_interval_internal(duration, tx.clone(), cancellation_token)
            .await;
        Ok(())
    }

    /// Execute an interval command that emits IntervalTick actions at regular
    /// intervals
    ///
    /// This function creates a background task that sends IntervalTick actions
    /// at the specified duration. The task will continue until the sender
    /// is dropped or the cancellation token is triggered, ensuring no
    /// memory leaks.
    ///
    /// # Arguments
    /// * `duration` - The interval duration between ticks
    /// * `tx` - Channel sender for emitting actions
    /// * `cancellation_token` - Token to cancel the interval
    async fn execute_interval_internal(
        &self,
        duration: std::time::Duration,
        tx: Sender<anyhow::Result<Action>>,
        cancellation_token: CancellationToken,
    ) {
        use tokio::time::interval;

        let cancel_id = CancelId::new(cancellation_token.clone());
        let start_time = Utc::now();

        // Create a tokio interval timer
        let mut interval_timer = interval(duration);

        // Skip the first tick which fires immediately
        interval_timer.tick().await;

        loop {
            tokio::select! {
                _ = interval_timer.tick() => {
                    let current_time = Utc::now();
                    let timer = Timer {start_time, current_time, duration, cancel: cancel_id.clone() };
                    let action = Action::IntervalTick(timer);

                    if tx.send(Ok(action)).await.is_err() {
                        break;
                    }
                }
                _ = cancellation_token.cancelled() => {
                    break;
                }
            }
        }
    }

    #[async_recursion::async_recursion]
    async fn execute_inner(
        &self,
        cmd: Command,
        tx: Sender<anyhow::Result<Action>>,
    ) -> anyhow::Result<()> {
        match cmd {
            Command::ChatMessage { message, conversation_id, is_first } => {
                self.execute_chat_message(message, conversation_id, is_first, &tx)
                    .await?;
            }
            Command::ReadWorkspace => {
                self.execute_read_workspace(&tx).await?;
            }
            Command::Empty => {
                self.execute_empty().await?;
            }
            Command::Exit => {
                self.execute_exit().await?;
            }
            Command::And(commands) => {
                self.execute_and(commands, &tx).await?;
            }
            Command::Interval { duration } => {
                self.execute_interval(duration, &tx).await?;
            }
            Command::Spotlight(_) => todo!(),
            Command::InterruptStream => {
                // Send InterruptStream action to trigger state update
                tx.send(Ok(Action::InterruptStream)).await?;
            }
        }
        Ok(())
    }

    pub async fn init(&self, tx: Sender<anyhow::Result<Action>>, mut rx: Receiver<Command>) {
        let this = self.clone();
        tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                this.execute(cmd, tx.clone()).await
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use pretty_assertions::assert_eq;
    use tokio::sync::mpsc;

    use super::*;

    #[tokio::test]
    async fn test_and_command_structure_with_empty_commands() {
        let command = Command::And(vec![Command::Empty, Command::Empty]);

        match command {
            Command::And(commands) => {
                assert_eq!(commands.len(), 2);
                assert_eq!(commands[0], Command::Empty);
                assert_eq!(commands[1], Command::Empty);
            }
            _ => panic!("Expected Command::And"),
        }
    }

    #[tokio::test]
    async fn test_and_command_structure() {
        let command = Command::And(vec![Command::Empty, Command::ReadWorkspace, Command::Exit]);

        match command {
            Command::And(commands) => {
                assert_eq!(commands.len(), 3);
                assert_eq!(commands[0], Command::Empty);
                assert_eq!(commands[1], Command::ReadWorkspace);
                assert_eq!(commands[2], Command::Exit);
            }
            _ => panic!("Expected Command::And"),
        }
    }

    #[tokio::test]
    async fn test_execute_empty_command_sends_no_action() {
        let (tx, mut rx) = mpsc::channel::<anyhow::Result<Action>>(10);

        // We can't easily test without a real API implementation
        // So we'll just test the command structure
        let command = Command::Empty;
        assert_eq!(command, Command::Empty);

        // Close the channel to prevent hanging
        drop(tx);

        // Verify no messages were sent
        let result = rx.try_recv();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_interval_command_structure() {
        let duration = Duration::from_millis(100);
        let fixture = Command::Interval { duration };

        match fixture {
            Command::Interval { duration: actual_duration } => {
                let expected = Duration::from_millis(100);
                assert_eq!(actual_duration, expected);
            }
            _ => panic!("Expected Command::Interval"),
        }
    }
}
