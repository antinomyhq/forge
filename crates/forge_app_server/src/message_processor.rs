use std::sync::Arc;

use anyhow::{Context, Result};
use forge_api::{ChatRequest, API};
use futures::StreamExt;
use tokio::io::AsyncWrite;
use uuid::Uuid;

use crate::protocol::{
    ClientRequest, InitializeResponse, JsonRpcRequest, ServerCapabilities, ServerNotification,
    TurnStatus,
};
use crate::{EventTranslator, OutgoingMessageSender};

/// Processes incoming JSON-RPC messages and dispatches to ForgeAPI
pub struct MessageProcessor<A, W: AsyncWrite + Unpin + Send> {
    api: Arc<A>,
    sender: OutgoingMessageSender<W>,
}

impl<A: API + 'static, W: AsyncWrite + Unpin + Send + 'static> MessageProcessor<A, W> {
    /// Creates a new MessageProcessor
    pub fn new(api: Arc<A>, sender: OutgoingMessageSender<W>) -> Self {
        Self { api, sender }
    }

    /// Processes a JSON-RPC message line
    pub async fn process_message(&self, line: &str) -> Result<()> {
        let request: JsonRpcRequest =
            serde_json::from_str(line).context("Failed to parse JSON-RPC request")?;

        // Handle the request based on method
        match request.method.as_str() {
            "initialize" => self.handle_initialize(&request).await,
            "initialized" => {
                tracing::info!("Client initialization complete");
                Ok(())
            }
            _ => self.handle_client_request(&request).await,
        }
    }

    async fn handle_initialize(&self, request: &JsonRpcRequest) -> Result<()> {
        let id = request.id.context("Initialize request must have an id")?;

        let response = InitializeResponse {
            capabilities: ServerCapabilities {
                user_agent: format!("forge-app-server/{}", env!("CARGO_PKG_VERSION")),
            },
        };

        self.sender.send_response(id, response).await?;
        Ok(())
    }

    async fn handle_client_request(&self, request: &JsonRpcRequest) -> Result<()> {
        let id = request.id.unwrap_or(-1);

        // Parse the client request
        let client_request: ClientRequest = if let Some(params) = &request.params {
            // Reconstruct the tagged enum format
            let mut value = serde_json::json!({
                "method": request.method.clone(),
            });
            if let Some(obj) = value.as_object_mut() {
                obj.insert("params".to_string(), params.clone());
            }
            serde_json::from_value(value).context(format!(
                "Failed to parse client request: {}",
                request.method
            ))?
        } else {
            serde_json::from_value(serde_json::json!({
                "method": request.method.clone(),
            }))?
        };

        // Dispatch to appropriate handler
        match client_request {
            ClientRequest::Initialize { .. } => {
                // Already handled above
                Ok(())
            }
            ClientRequest::Initialized => Ok(()),
            ClientRequest::ThreadStart { thread_id, agent_id } => {
                self.handle_thread_start(id, thread_id, agent_id).await
            }
            ClientRequest::ThreadList { limit } => self.handle_thread_list(id, limit).await,
            ClientRequest::ThreadGet { thread_id } => self.handle_thread_get(id, thread_id).await,
            ClientRequest::TurnStart { thread_id, turn_id, message, files } => {
                self.handle_turn_start(id, thread_id, turn_id, message, files)
                    .await
            }
            ClientRequest::TurnRetry { thread_id } => self.handle_turn_retry(id, thread_id).await,
            ClientRequest::ThreadCompact { thread_id } => {
                self.handle_thread_compact(id, thread_id).await
            }
            ClientRequest::AgentSet { agent_id } => self.handle_agent_set(id, agent_id).await,
            ClientRequest::AgentList => self.handle_agent_list(id).await,
            ClientRequest::ModelList => self.handle_model_list(id).await,
            ClientRequest::ModelSet { model_id } => self.handle_model_set(id, model_id).await,
            ClientRequest::ProviderList => self.handle_provider_list(id).await,
            ClientRequest::ProviderSet { provider_id } => {
                self.handle_provider_set(id, provider_id).await
            }
            ClientRequest::GitCommit { preview, max_diff_size, additional_context } => {
                self.handle_git_commit(id, preview, max_diff_size, additional_context)
                    .await
            }
            ClientRequest::CommandSuggest { prompt } => {
                self.handle_command_suggest(id, prompt).await
            }
            ClientRequest::SkillList => self.handle_skill_list(id).await,
            ClientRequest::CommandList => self.handle_command_list(id).await,
            ClientRequest::EnvInfo => self.handle_env_info(id).await,
            ClientRequest::ApprovalFileChange { .. } => {
                // TODO: Implement approval handling
                self.sender
                    .send_error(id, -32601, "Approval handling not yet implemented")
                    .await
            }
            ClientRequest::ApprovalCommandExecution { .. } => {
                // TODO: Implement approval handling
                self.sender
                    .send_error(id, -32601, "Approval handling not yet implemented")
                    .await
            }
        }
    }

    async fn handle_thread_start(
        &self,
        id: i64,
        thread_id: Option<Uuid>,
        agent_id: Option<String>,
    ) -> Result<()> {
        // Set active agent if provided
        if let Some(agent) = agent_id {
            self.api
                .set_active_agent(agent.as_str().into())
                .await
                .context("Failed to set active agent")?;
        }

        // Get or create conversation
        let conversation_id: forge_domain::ConversationId = if let Some(tid) = thread_id {
            forge_domain::ConversationId::parse(tid.to_string())?
        } else {
            // Create new conversation by generating a new ID
            forge_domain::ConversationId::generate()
        };

        // Actually create the conversation in ForgeAPI
        // This ensures the conversation exists before turn/start is called
        let conversation = forge_domain::Conversation::new(conversation_id);
        self.api
            .upsert_conversation(conversation)
            .await
            .context("Failed to create conversation")?;

        // Send thread started notification
        self.sender
            .send_notification(ServerNotification::ThreadStarted {
                thread_id: Uuid::parse_str(&conversation_id.into_string())?,
            })
            .await?;

        // Send success response
        self.sender
            .send_response(
                id,
                serde_json::json!({"threadId": conversation_id.into_string()}),
            )
            .await
    }

    async fn handle_thread_list(&self, id: i64, limit: Option<usize>) -> Result<()> {
        let conversations = self
            .api
            .get_conversations(limit)
            .await
            .context("Failed to get conversations")?;

        let threads: Vec<serde_json::Value> = conversations
            .iter()
            .map(|conv| {
                serde_json::json!({
                    "threadId": conv.id,
                    "title": conv.title,
                    "createdAt": conv.metadata.created_at,
                    "updatedAt": conv.metadata.updated_at,
                    "messageCount": conv.context.as_ref().map(|c| c.messages.len()).unwrap_or(0),
                })
            })
            .collect();

        self.sender
            .send_response(id, serde_json::json!({"threads": threads}))
            .await
    }

    async fn handle_thread_get(&self, id: i64, thread_id: Uuid) -> Result<()> {
        let conversation_id = forge_domain::ConversationId::parse(thread_id.to_string())?;
        let conversation = self
            .api
            .conversation(&conversation_id)
            .await
            .context("Failed to get conversation")?;

        if let Some(conv) = conversation {
            self.sender
                .send_response(
                    id,
                    serde_json::json!({
                        "threadId": conv.id,
                        "title": conv.title,
                        "createdAt": conv.metadata.created_at,
                        "updatedAt": conv.metadata.updated_at,
                        "messageCount": conv.context.as_ref().map(|c| c.messages.len()).unwrap_or(0),
                    }),
                )
                .await
        } else {
            self.sender.send_error(id, -32602, "Thread not found").await
        }
    }

    async fn handle_turn_start(
        &self,
        id: i64,
        thread_id: Uuid,
        turn_id: Uuid,
        message: String,
        _files: Option<Vec<String>>,
    ) -> Result<()> {
        // Send turn started notification
        self.sender
            .send_notification(ServerNotification::TurnStarted { thread_id, turn_id })
            .await?;

        // Send immediate success response (streaming will happen via notifications)
        self.sender
            .send_response(id, serde_json::json!({"success": true}))
            .await?;

        // Start chat in background
        let api = self.api.clone();
        let sender = self.sender.clone();

        tokio::spawn(async move {
            let result = Self::execute_chat(api, sender, thread_id, turn_id, message).await;
            if let Err(e) = result {
                tracing::error!("Chat execution failed: {}", e);
            }
        });

        Ok(())
    }

    async fn execute_chat(
        api: Arc<A>,
        sender: OutgoingMessageSender<W>,
        thread_id: Uuid,
        turn_id: Uuid,
        message: String,
    ) -> Result<()> {
        // Create chat request
        let conversation_id = forge_domain::ConversationId::parse(thread_id.to_string())?;
        let chat_request =
            ChatRequest { event: forge_domain::Event::new(message), conversation_id };

        // Get chat stream
        let mut stream = api.chat(chat_request).await?;

        // Create translator
        let mut translator = EventTranslator::new(thread_id, turn_id);

        // Process stream
        while let Some(response_result) = stream.next().await {
            match response_result {
                Ok(response) => {
                    let notifications = translator.translate(response);
                    for notification in notifications {
                        if let Err(e) = sender.send_notification(notification).await {
                            tracing::error!("Failed to send notification: {}", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Chat stream error: {}", e);
                    // Send turn failed notification
                    sender
                        .send_notification(ServerNotification::TurnCompleted {
                            thread_id,
                            turn_id,
                            status: TurnStatus::Failed,
                        })
                        .await?;
                    break;
                }
            }
        }

        Ok(())
    }

    async fn handle_turn_retry(&self, id: i64, _thread_id: Uuid) -> Result<()> {
        // TODO: Implement retry logic
        self.sender
            .send_error(id, -32601, "Retry not yet implemented")
            .await
    }

    async fn handle_thread_compact(&self, id: i64, thread_id: Uuid) -> Result<()> {
        let conversation_id = forge_domain::ConversationId::parse(thread_id.to_string())?;
        let result = self
            .api
            .compact_conversation(&conversation_id)
            .await
            .context("Failed to compact conversation")?;

        self.sender
            .send_response(
                id,
                serde_json::json!({
                    "originalTokens": result.original_tokens,
                    "compactedTokens": result.compacted_tokens,
                    "originalMessages": result.original_messages,
                    "compactedMessages": result.compacted_messages,
                }),
            )
            .await
    }

    async fn handle_agent_set(&self, id: i64, agent_id: String) -> Result<()> {
        self.api
            .set_active_agent(agent_id.as_str().into())
            .await
            .context("Failed to set active agent")?;

        self.sender
            .send_response(id, serde_json::json!({"success": true}))
            .await
    }

    async fn handle_agent_list(&self, id: i64) -> Result<()> {
        let agents = self
            .api
            .get_agents()
            .await
            .context("Failed to get agents")?;

        let agent_list: Vec<serde_json::Value> = agents
            .iter()
            .map(|agent| {
                serde_json::json!({
                    "id": agent.id,
                    "provider": agent.provider,
                    "model": agent.model,
                })
            })
            .collect();

        self.sender
            .send_response(id, serde_json::json!({"agents": agent_list}))
            .await
    }

    async fn handle_model_list(&self, id: i64) -> Result<()> {
        let models = self
            .api
            .get_models()
            .await
            .context("Failed to get models")?;

        let model_list: Vec<serde_json::Value> = models
            .iter()
            .map(|model| {
                serde_json::json!({
                    "id": model.id,
                    "name": model.name,
                    "contextLength": model.context_length,
                })
            })
            .collect();

        self.sender
            .send_response(id, serde_json::json!({"models": model_list}))
            .await
    }

    async fn handle_model_set(&self, id: i64, model_id: String) -> Result<()> {
        self.api
            .set_default_model(model_id.into())
            .await
            .context("Failed to set default model")?;

        self.sender
            .send_response(id, serde_json::json!({"success": true}))
            .await
    }

    async fn handle_provider_list(&self, id: i64) -> Result<()> {
        let providers = self
            .api
            .get_providers()
            .await
            .context("Failed to get providers")?;

        let active_provider = self
            .api
            .get_default_provider()
            .await
            .ok()
            .map(|p| p.id.to_string());

        let provider_list: Vec<serde_json::Value> = providers
            .iter()
            .map(|provider| {
                let provider_id = provider.id().to_string();
                let is_active = active_provider
                    .as_ref() == Some(&provider_id);
                serde_json::json!({
                    "id": provider_id,
                    "isActive": is_active,
                })
            })
            .collect();

        self.sender
            .send_response(id, serde_json::json!({"providers": provider_list}))
            .await
    }

    async fn handle_provider_set(&self, id: i64, provider_id: String) -> Result<()> {
        // Parse provider_id from string using serde
        let provider_id: forge_domain::ProviderId =
            serde_json::from_str(&format!("\"{}\"", provider_id)).context("Invalid provider ID")?;

        self.api
            .set_default_provider(provider_id)
            .await
            .context("Failed to set default provider")?;

        self.sender
            .send_response(id, serde_json::json!({"success": true}))
            .await
    }

    async fn handle_git_commit(
        &self,
        id: i64,
        preview: bool,
        max_diff_size: Option<usize>,
        additional_context: Option<String>,
    ) -> Result<()> {
        let result = self
            .api
            .commit(preview, max_diff_size, None, additional_context)
            .await
            .context("Failed to generate commit message")?;

        self.sender
            .send_response(
                id,
                serde_json::json!({
                    "message": result.message,
                    "hasStagedFiles": result.has_staged_files,
                }),
            )
            .await
    }

    async fn handle_command_suggest(&self, id: i64, prompt: String) -> Result<()> {
        let command = self
            .api
            .generate_command(prompt.into())
            .await
            .context("Failed to generate command")?;

        self.sender
            .send_response(id, serde_json::json!({"command": command}))
            .await
    }

    async fn handle_skill_list(&self, id: i64) -> Result<()> {
        let skills = self
            .api
            .get_skills()
            .await
            .context("Failed to get skills")?;

        let skill_list: Vec<serde_json::Value> = skills
            .iter()
            .map(|skill| {
                serde_json::json!({
                    "name": skill.name,
                    "description": skill.description,
                })
            })
            .collect();

        self.sender
            .send_response(id, serde_json::json!({"skills": skill_list}))
            .await
    }

    async fn handle_command_list(&self, id: i64) -> Result<()> {
        let commands = self
            .api
            .get_commands()
            .await
            .context("Failed to get commands")?;

        let command_list: Vec<serde_json::Value> = commands
            .iter()
            .map(|cmd| {
                serde_json::json!({
                    "name": cmd.name,
                    "description": cmd.description,
                })
            })
            .collect();

        self.sender
            .send_response(id, serde_json::json!({"commands": command_list}))
            .await
    }

    async fn handle_env_info(&self, id: i64) -> Result<()> {
        let env = self.api.environment();
        let active_agent = self.api.get_active_agent().await;
        let default_model = self.api.get_default_model().await;

        self.sender
            .send_response(
                id,
                serde_json::json!({
                    "cwd": env.cwd,
                    "os": env.os,
                    "shell": env.shell,
                    "home": env.home,
                    "activeAgent": active_agent,
                    "defaultModel": default_model,
                }),
            )
            .await
    }
}
