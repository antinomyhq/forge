use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::Result;
use derive_setters::Setters;
use forge_domain::{
    ChatCompletionMessage, ChatRequest, Context, FileReadService, Model, ModelId, Parameters,
    ProviderService, ResultStream,
};
use tokio_stream::StreamExt;

use crate::service::PromptService;

#[derive(Default)]
pub struct TestFileReadService(BTreeMap<String, String>);

impl TestFileReadService {
    pub fn add(mut self, path: impl ToString, content: impl ToString) -> Self {
        self.0.insert(path.to_string(), content.to_string());
        self
    }
}

#[async_trait::async_trait]
impl FileReadService for TestFileReadService {
    async fn read(&self, path: PathBuf) -> Result<String> {
        let path_str = path.to_string_lossy().to_string();
        self.0.get(&path_str).cloned().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("File not found: {}", path_str),
            )
            .into()
        })
    }
}

#[derive(Default)]
pub struct TestPrompt {
    system: Option<String>,
}

impl TestPrompt {
    pub fn new(system: impl Into<String>) -> Self {
        Self { system: Some(system.into()) }
    }
}

#[async_trait::async_trait]
impl PromptService for TestPrompt {
    async fn get(&self, request: &ChatRequest) -> Result<String> {
        let content = match self.system.clone() {
            None => format!("<task>{}</task>", request.content),
            Some(prompt) => prompt,
        };

        Ok(content)
    }
}

#[derive(Default, Setters)]
pub struct TestProvider {
    pub(crate) messages: Mutex<Vec<Vec<ChatCompletionMessage>>>,
    pub(crate) calls: Mutex<Vec<Context>>,
    pub(crate) models: Vec<Model>,
    pub(crate) parameters: Vec<(ModelId, Parameters)>,
}

impl TestProvider {
    pub fn with_messages(self, messages: Vec<Vec<ChatCompletionMessage>>) -> Self {
        self.messages(Mutex::new(messages))
    }

    pub fn message(&self) -> usize {
        self.messages.lock().unwrap().len()
    }

    pub fn get_calls(&self) -> Vec<Context> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl ProviderService for TestProvider {
    async fn chat(
        &self,
        _model_id: &ModelId,
        request: Context,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        self.calls.lock().unwrap().push(request);
        let mut guard = self.messages.lock().unwrap();
        if guard.is_empty() {
            Ok(Box::pin(tokio_stream::empty()))
        } else {
            let response = guard.remove(0);
            Ok(Box::pin(tokio_stream::iter(response).map(Ok)))
        }
    }

    async fn models(&self) -> Result<Vec<Model>> {
        Ok(self.models.clone())
    }

    async fn parameters(&self, model: &ModelId) -> Result<Parameters> {
        match self.parameters.iter().find(|(id, _)| id == model) {
            None => anyhow::bail!("Model not found: {}", model),
            Some((_, parameter)) => Ok(parameter.clone()),
        }
    }
}
