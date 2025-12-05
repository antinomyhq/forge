use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use forge_domain::{Context, ContextMessage, DataGenerationParameters, Template, ToolDefinition};
use futures::StreamExt;
use futures::stream::{self, BoxStream};
use schemars::schema::RootSchema;

use crate::{
    AppConfigService, EnvironmentService, FsReadService, ProviderService, Services, TemplateEngine,
};

pub struct DataGenerationApp<A> {
    services: Arc<A>,
}

type JsonSchema = String;
type SystemPrompt = String;
type UserPrompt = String;
type Input = Vec<serde_json::Value>;

impl<A: Services> DataGenerationApp<A> {
    pub fn new(services: Arc<A>) -> Self {
        Self { services }
    }

    /// Helper function to read a file from a path, resolving it relative to cwd
    /// if necessary
    async fn read_file(&self, path: PathBuf) -> Result<String> {
        let resolved_path = if path.is_absolute() {
            path
        } else {
            let cwd = self.services.get_environment().cwd;
            cwd.join(path)
        };

        let content = self
            .services
            .read(resolved_path.display().to_string(), None, None)
            .await?
            .content
            .file_content()
            .to_owned();

        Ok(content)
    }

    async fn read_file_opt(&self, path: Option<PathBuf>) -> Result<Option<String>> {
        match path {
            Some(path) => self.read_file(path).await.map(Some),
            None => Ok(None),
        }
    }

    async fn load_parameters(
        &self,
        params: DataGenerationParameters,
    ) -> Result<(JsonSchema, Option<SystemPrompt>, Option<UserPrompt>, Input)> {
        // Read all files in parallel
        let (schema, system_prompt, user_prompt, input) = tokio::join!(
            self.read_file(params.schema.clone()),
            self.read_file_opt(params.system_prompt),
            self.read_file_opt(params.user_prompt),
            self.read_file(params.input)
        );

        let input: Vec<serde_json::Value> = input?
            .split("\n")
            .map(|text| Ok(serde_json::from_str(text)?))
            .collect::<Result<Vec<_>>>()?;

        Ok((schema?, system_prompt?, user_prompt?, input))
    }

    pub async fn execute(
        &self,
        params: DataGenerationParameters,
    ) -> Result<BoxStream<'static, Result<serde_json::Value>>> {
        let concurrency = params.concurrency;
        let (schema, system_prompt, user_prompt, input) = self.load_parameters(params).await?;
        let provider = self.services.get_default_provider().await?;
        let model_id = self.services.get_provider_model(Some(&provider.id)).await?;
        let schema: RootSchema = serde_json::from_str(&schema)?;
        let mut context =
            Context::default().add_tool(ToolDefinition::new("output").input_schema(schema));

        if let Some(content) = system_prompt {
            context = context.add_message(ContextMessage::system(content))
        }

        let services = self.services.clone();

        let json_stream = input.into_iter().map(move |data| {
            let provider = provider.clone();
            let context = context.clone();
            let user_prompt = user_prompt.clone();
            let model_id = model_id.clone();
            let services = services.clone();

            async move {
                let provider = provider.clone();
                let mut context = context.clone();
                let content = if let Some(ref content) = user_prompt {
                    TemplateEngine::default().render_template(Template::new(content), &data)?
                } else {
                    serde_json::to_string(&data)?
                };

                context =
                    context.add_message(ContextMessage::user(content, Some(model_id.clone())));

                let stream = services.chat(&model_id, context, provider.clone()).await?;

                anyhow::Ok(stream)
            }
        });

        let json_stream = stream::iter(json_stream)
            .buffer_unordered(concurrency)
            .map(|data| match data {
                Ok(data) => data,
                Err(err) => Box::pin(stream::once(async { Err(err) })),
            })
            .flatten()
            .filter_map(|result| async {
                result.map(|data| data.content).transpose().map(|result| {
                    result.and_then(|content| Ok(serde_json::from_str(content.as_str())?))
                })
            });

        Ok(json_stream.boxed())
    }
}
