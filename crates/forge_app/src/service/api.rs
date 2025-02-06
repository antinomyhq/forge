use std::sync::Arc;

use anyhow::Result;
use forge_domain::{
    ChatRequest, ChatResponse, Config, ConfigRepository, Context, Conversation,
    ConversationHistory, ConversationId, ConversationRepository, Environment, Model,
    ProviderService, ResultStream, ToolDefinition, ToolService,
};

use super::suggestion::{File, SuggestionService};
use super::ui::UIService;
use super::Service;

#[async_trait::async_trait]
pub trait APIService: Send + Sync {
    async fn suggestions(&self) -> Result<Vec<File>>;
    async fn tools(&self) -> Vec<ToolDefinition>;
    async fn context(&self, conversation_id: ConversationId) -> Result<Context>;
    async fn models(&self) -> Result<Vec<Model>>;
    async fn chat(&self, chat: ChatRequest) -> ResultStream<ChatResponse, anyhow::Error>;
    async fn conversations(&self) -> Result<Vec<Conversation>>;
    async fn conversation(&self, conversation_id: ConversationId) -> Result<ConversationHistory>;
    async fn get_config(&self) -> Result<Config>;
    async fn set_config(&self, request: Config) -> Result<Config>;
    async fn environment(&self) -> Result<Environment>;
}

impl Service {
    pub fn api_service(env: Environment) -> Result<impl APIService> {
        Live::new(env)
    }
}

#[derive(Clone)]
struct Live {
    provider: Arc<dyn ProviderService>,
    tool: Arc<dyn ToolService>,
    completions: Arc<dyn SuggestionService>,
    ui_service: Arc<dyn UIService>,
    conversation_repo: Arc<dyn ConversationRepository>,
    config_repo: Arc<dyn ConfigRepository>,
    environment: Environment,
}

impl Live {
    fn new(env: Environment) -> Result<Self> {
        let provider = Arc::new(Service::provider_service(
            env.api_key.clone(),
            env.base_url.clone(),
            env.provider.clone(),
        ));
        let tool = Arc::new(Service::tool_service());
        let file_read = Arc::new(Service::file_read_service());

        let system_prompt = Arc::new(Service::system_prompt(
            env.clone(),
            tool.clone(),
            provider.clone(),
            file_read.clone(),
        ));

        let user_prompt = Arc::new(Service::user_prompt_service());

        // Create an owned String that will live for 'static
        let sqlite = Arc::new(Service::db_pool_service(env.db_path()));

        let conversation_repo = Arc::new(Service::conversation_repo(sqlite.clone()));

        let config_repo = Arc::new(Service::config_repo(sqlite.clone()));

        let chat_service = Arc::new(Service::chat_service(
            provider.clone(),
            system_prompt.clone(),
            tool.clone(),
            user_prompt,
        ));
        // Use the environment's cwd for completions since that's always available
        let completions = Arc::new(Service::completion_service(env.cwd.clone()));

        let title_service = Arc::new(Service::title_service(provider.clone()));

        let chat_service = Arc::new(Service::ui_service(
            conversation_repo.clone(),
            chat_service,
            title_service,
        ));

        Ok(Self {
            provider,
            tool,
            completions,
            ui_service: chat_service,
            conversation_repo,
            config_repo,
            environment: env,
        })
    }
}

#[async_trait::async_trait]
impl APIService for Live {
    async fn suggestions(&self) -> Result<Vec<File>> {
        self.completions.suggestions().await
    }

    async fn tools(&self) -> Vec<ToolDefinition> {
        self.tool.list()
    }

    async fn context(&self, conversation_id: ConversationId) -> Result<Context> {
        Ok(self.conversation_repo.get(conversation_id).await?.context)
    }

    async fn models(&self) -> Result<Vec<Model>> {
        Ok(self.provider.models().await?)
    }

    async fn chat(&self, chat: ChatRequest) -> ResultStream<ChatResponse, anyhow::Error> {
        Ok(self.ui_service.chat(chat).await?)
    }

    async fn conversations(&self) -> Result<Vec<Conversation>> {
        self.conversation_repo.list().await
    }

    async fn conversation(&self, conversation_id: ConversationId) -> Result<ConversationHistory> {
        Ok(self
            .conversation_repo
            .get(conversation_id)
            .await?
            .context
            .into())
    }

    async fn get_config(&self) -> Result<Config> {
        Ok(self.config_repo.get().await?)
    }

    async fn set_config(&self, config: Config) -> Result<Config> {
        self.config_repo.set(config).await
    }

    async fn environment(&self) -> Result<Environment> {
        Ok(self.environment.clone())
    }
}
