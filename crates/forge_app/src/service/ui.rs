use std::sync::Arc;

use forge_domain::{
    ChatRequest, ChatResponse, Context, ContextMessage, Conversation, ConversationId,
    ConversationRepository, ModelId, ResultStream,
};
use tokio_stream::{once, StreamExt};
use tracing::debug;

use super::chat::ChatService;
use super::workflow_title::TitleService;
use super::Service;

#[async_trait::async_trait]
pub trait UIService: Send + Sync {
    async fn retry(
        &self,
        conversation_id: ConversationId,
        model_id: ModelId,
    ) -> ResultStream<ChatResponse, anyhow::Error>;
    async fn chat(&self, request: ChatRequest) -> ResultStream<ChatResponse, anyhow::Error>;
}

struct Live {
    conversation_service: Arc<dyn ConversationRepository>,
    chat_service: Arc<dyn ChatService>,
    title_service: Arc<dyn TitleService>,
}

impl Live {
    fn new(
        conversation_service: Arc<dyn ConversationRepository>,
        chat_service: Arc<dyn ChatService>,
        title_service: Arc<dyn TitleService>,
    ) -> Self {
        Self { conversation_service, chat_service, title_service }
    }
}

impl Service {
    pub fn ui_service(
        conversation_service: Arc<dyn ConversationRepository>,
        neo_chat_service: Arc<dyn ChatService>,
        title_service: Arc<dyn TitleService>,
    ) -> impl UIService {
        Live::new(conversation_service, neo_chat_service, title_service)
    }
}

impl Live {
    pub async fn execute(
        &self,
        request: ChatRequest,
        conversation: Conversation,
        is_new: bool,
    ) -> ResultStream<ChatResponse, anyhow::Error> {
        let mut stream = self
            .chat_service
            .chat(request.clone(), conversation.context)
            .await?;

        if is_new {
            let title_stream = self.title_service.get_title(request.clone()).await?;
            let id = conversation.id;
            stream = Box::pin(
                once(Ok(ChatResponse::ConversationStarted(id)))
                    .chain(title_stream)
                    .merge(stream),
            );
        }

        let conversation_service = self.conversation_service.clone();
        let stream = stream
            .then(move |message| {
                let conversation_service = conversation_service.clone();
                async move {
                    match &message {
                        Ok(ChatResponse::CompleteTitle(title)) => {
                            let conversation_id = request
                                .conversation_id
                                .expect("`conversation_id` must be set at this point.");
                            conversation_service
                                .set_title(&conversation_id, title.to_owned())
                                .await?;
                            message
                        }
                        Ok(ChatResponse::ModifyContext(context)) => {
                            conversation_service
                                .insert(context, request.conversation_id)
                                .await?;
                            message
                        }
                        _ => message,
                    }
                }
            })
            .filter(|message| !matches!(message, Ok(ChatResponse::ModifyContext { .. })));

        Ok(Box::pin(stream))
    }

    // finds the last user message in the context
    fn find_last_user_message(context: &Context) -> Option<usize> {
        context.messages.iter().enumerate().rev()
            .find(|(_, msg)| {
                matches!(msg, ContextMessage::ContentMessage(content_msg) if content_msg.role == forge_domain::Role::User)
            }).map(|(index, _)| index)
    }
}

#[async_trait::async_trait]
impl UIService for Live {
    async fn retry(
        &self,
        conversation_id: ConversationId,
        model_id: ModelId,
    ) -> ResultStream<ChatResponse, anyhow::Error> {
        let mut conversation = self.conversation_service.get(conversation_id).await?;

        // find the last user message in the context
        let last_index = Self::find_last_user_message(&conversation.context)
            .ok_or_else(|| anyhow::anyhow!("No user message found to retry"))?;

        // Truncate to remove messages after the last user message
        conversation.context.messages.truncate(last_index + 1);

        // Create request with context up to the last user message
        let request = ChatRequest::new(model_id).conversation_id(conversation_id);
        self.execute(request, conversation, false).await
    }

    async fn chat(&self, request: ChatRequest) -> ResultStream<ChatResponse, anyhow::Error> {
        let (conversation, is_new) = if let Some(conversation_id) = &request.conversation_id {
            let context = self.conversation_service.get(*conversation_id).await?;
            (context, false)
        } else {
            let conversation = self
                .conversation_service
                .insert(&Context::default(), None)
                .await?;
            (conversation, true)
        };

        debug!("Job {} started", conversation.id);
        let request = request.conversation_id(conversation.id);
        self.execute(request, conversation, is_new).await
    }
}

#[cfg(test)]
mod tests {
    use core::panic;
    use std::sync::Mutex;

    use forge_domain::{ModelId, Role};
    use pretty_assertions::assert_eq;
    use tokio_stream::StreamExt;

    use super::*;
    use crate::repo::test::TestConversationStorage;

    struct TestTitleService {
        events: Vec<ChatResponse>,
    }

    impl TestTitleService {
        fn single() -> Self {
            Self {
                events: vec![ChatResponse::CompleteTitle(
                    "test title generated".to_string(),
                )],
            }
        }
    }

    #[async_trait::async_trait]
    impl TitleService for TestTitleService {
        async fn get_title(&self, _: ChatRequest) -> ResultStream<ChatResponse, anyhow::Error> {
            Ok(Box::pin(tokio_stream::iter(
                self.events.clone().into_iter().map(Ok),
            )))
        }
    }

    struct TestChatService {
        events: Vec<ChatResponse>,
        request: Mutex<Option<ChatRequest>>,
        context: Mutex<Option<Context>>,
    }

    impl TestChatService {
        fn single() -> Self {
            Self {
                events: vec![ChatResponse::Text("test message".to_string())],
                request: Mutex::new(None),
                context: Mutex::new(None),
            }
        }
    }

    #[async_trait::async_trait]
    impl ChatService for TestChatService {
        async fn chat(
            &self,
            request: ChatRequest,
            context: Context,
        ) -> ResultStream<ChatResponse, anyhow::Error> {
            self.request.lock().unwrap().replace(request.clone());
            self.context.lock().unwrap().replace(context.clone());
            Ok(Box::pin(tokio_stream::iter(
                self.events.clone().into_iter().map(Ok),
            )))
        }
    }

    #[tokio::test]
    async fn test_chat_existing_conversation() {
        let conversation_service = Arc::new(TestConversationStorage::in_memory().unwrap());
        let service = Service::ui_service(
            conversation_service.clone(),
            Arc::new(TestChatService::single()),
            Arc::new(TestTitleService::single()),
        );

        let model_id = ModelId::new("gpt-3.5-turbo");
        let conversation = conversation_service
            .insert(&Context::default(), None)
            .await
            .unwrap();

        let request = ChatRequest::new(model_id)
            .content("test")
            .conversation_id(conversation.id);
        let mut responses = service.chat(request).await.unwrap();

        if let Some(Ok(ChatResponse::Text(content))) = responses.next().await {
            assert_eq!(content, "test message");
        } else {
            panic!("Expected Text response");
        }

        assert!(responses.next().await.is_none(), "Expected end of stream");
    }

    #[tokio::test]
    async fn test_retry_with_user_message() {
        let conversation_service = Arc::new(TestConversationStorage::in_memory().unwrap());
        let model_id = ModelId::new("gpt-3.5-turbo");

        // Create a conversation with a user message
        let mut context = Context::default();
        context.messages.push(ContextMessage::ContentMessage(
            forge_domain::ContentMessage {
                role: Role::User,
                content: "hey there".to_string(),
                tool_calls: None,
            },
        ));
        context.messages.push(ContextMessage::ContentMessage(
            forge_domain::ContentMessage {
                role: Role::Assistant,
                content: "hi, i'm code forge an coding assistant. how can i help you".to_string(),
                tool_calls: None,
            },
        ));
        context.messages.push(ContextMessage::ContentMessage(
            forge_domain::ContentMessage {
                role: Role::User,
                content: "what's 2 + 2 = ?".to_string(),
                tool_calls: None,
            },
        ));

        let conversation = conversation_service.insert(&context, None).await.unwrap();
        let chat_svc = Arc::new(TestChatService::single());
        let service = Service::ui_service(
            conversation_service.clone(),
            chat_svc.clone(),
            Arc::new(TestTitleService::single()),
        );
        let mut responses = service.retry(conversation.id, model_id).await.unwrap();

        if let Some(Ok(ChatResponse::Text(content))) = responses.next().await {
            assert_eq!(content, "test message");
        } else {
            panic!("Expected Text response");
        }

        assert_eq!(
            chat_svc.context.lock().unwrap().clone().unwrap().clone(),
            context.clone()
        );
        assert!(chat_svc
            .request
            .lock()
            .unwrap()
            .clone()
            .unwrap()
            .content
            .is_none());
    }

    #[tokio::test]
    async fn test_retry_with_last_user_message() {
        let conversation_service = Arc::new(TestConversationStorage::in_memory().unwrap());
        let model_id = ModelId::new("gpt-3.5-turbo");

        // Create a conversation with a user message
        let mut context = Context::default();
        context.messages.push(ContextMessage::ContentMessage(
            forge_domain::ContentMessage {
                role: Role::User,
                content: "hey there".to_string(),
                tool_calls: None,
            },
        ));
        context.messages.push(ContextMessage::ContentMessage(
            forge_domain::ContentMessage {
                role: Role::Assistant,
                content: "hi, i'm code forge an coding assistant. how can i help you".to_string(),
                tool_calls: None,
            },
        ));

        let conversation = conversation_service.insert(&context, None).await.unwrap();
        let chat_svc = Arc::new(TestChatService::single());
        let service = Service::ui_service(
            conversation_service.clone(),
            chat_svc.clone(),
            Arc::new(TestTitleService::single()),
        );
        let mut responses = service.retry(conversation.id, model_id).await.unwrap();

        if let Some(Ok(ChatResponse::Text(content))) = responses.next().await {
            assert_eq!(content, "test message");
        } else {
            panic!("Expected Text response");
        }

        let mut expected_context = Context::default();
        expected_context
            .messages
            .push(ContextMessage::ContentMessage(
                forge_domain::ContentMessage {
                    role: Role::User,
                    content: "hey there".to_string(),
                    tool_calls: None,
                },
            ));

        assert_eq!(
            chat_svc.context.lock().unwrap().clone().unwrap().clone(),
            expected_context
        );
        assert!(chat_svc
            .request
            .lock()
            .unwrap()
            .clone()
            .unwrap()
            .content
            .is_none());
    }

    #[tokio::test]
    async fn test_retry_with_no_user_message() {
        let conversation_service = Arc::new(TestConversationStorage::in_memory().unwrap());
        let model_id = ModelId::new("gpt-3.5-turbo");

        // Create a conversation with a user message
        let mut context = Context::default();
        context.messages.push(ContextMessage::ContentMessage(
            forge_domain::ContentMessage {
                role: Role::Assistant,
                content: "hi, i'm code forge an coding assistant. how can i help you".to_string(),
                tool_calls: None,
            },
        ));

        let conversation = conversation_service.insert(&context, None).await.unwrap();
        let chat_svc = Arc::new(TestChatService::single());
        let service = Service::ui_service(
            conversation_service.clone(),
            chat_svc.clone(),
            Arc::new(TestTitleService::single()),
        );
        let responses = service.retry(conversation.id, model_id).await;

        if let Err(e) = responses {
            assert_eq!(e.to_string(), "No user message found to retry");
        } else {
            panic!("Expected error");
        }
    }

    #[tokio::test]
    async fn test_retry_with_empty_context() {
        let conversation_service = Arc::new(TestConversationStorage::in_memory().unwrap());
        let model_id = ModelId::new("gpt-3.5-turbo");

        // Create a conversation with a user message
        let context = Context::default();
        let conversation = conversation_service.insert(&context, None).await.unwrap();
        let chat_svc = Arc::new(TestChatService::single());
        let service = Service::ui_service(
            conversation_service.clone(),
            chat_svc.clone(),
            Arc::new(TestTitleService::single()),
        );
        let responses = service.retry(conversation.id, model_id).await;

        if let Err(e) = responses {
            assert_eq!(e.to_string(), "No user message found to retry");
        } else {
            panic!("Expected error");
        }
    }
}
