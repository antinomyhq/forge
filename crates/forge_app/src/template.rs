use std::collections::HashMap;
use std::sync::Arc;

use forge_domain::{
    Agent, Event, EventContext, Query, SystemContext, Template, TemplateService, ToolService,
};
use forge_walker::Walker;
use handlebars::Handlebars;
use rust_embed::Embed;
use serde_json::Value;
use tracing::debug;

use crate::{EmbeddingService, EnvironmentService, Infrastructure, VectorIndex};

#[derive(Embed)]
#[folder = "../../templates/"]
struct Templates;

pub struct ForgeTemplateService<F, T> {
    hb: Handlebars<'static>,
    infra: Arc<F>,
    tool_service: Arc<T>,
}

impl<F, T> ForgeTemplateService<F, T> {
    pub fn new(infra: Arc<F>, tool_service: Arc<T>) -> Self {
        let mut hb = Handlebars::new();
        hb.set_strict_mode(true);
        hb.register_escape_fn(|str| str.to_string());

        // Register all partial templates
        hb.register_embed_templates::<Templates>().unwrap();

        Self { hb, infra, tool_service }
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure, T: ToolService> TemplateService for ForgeTemplateService<F, T> {
    async fn render_system(
        &self,
        agent: &Agent,
        prompt: &Template<SystemContext>,
    ) -> anyhow::Result<String> {
        let env = self.infra.environment_service().get_environment();

        // Build the walker, only setting max_depth if a value was provided
        let mut walker = Walker::max_all();

        // Only set max_depth if the value is provided
        walker = walker.max_depth(agent.max_walker_depth.unwrap_or(1));

        let mut files = walker
            .cwd(env.cwd.clone())
            .get()
            .await?
            .iter()
            .map(|f| f.path.to_string())
            .collect::<Vec<_>>();

        // Sort the files alphabetically to ensure consistent ordering
        files.sort();

        let ctx = SystemContext {
            env: Some(env),
            tool_information: Some(self.tool_service.usage_prompt()),
            tool_supported: Some(true),
            files,
        };

        Ok(self.hb.render_template(prompt.template.as_str(), &ctx)?)
    }

    async fn render_event(
        &self,
        agent: &Agent,
        prompt: &Template<EventContext>,
        event: &Event,
        variables: &HashMap<String, Value>,
    ) -> anyhow::Result<String> {
        // Create an EventContext with the provided event
        let mut event_context = EventContext::new(event.clone());

        // Add variables to the context
        event_context = event_context.variables(variables.clone());

        // Only add suggestions if the agent has suggestions enabled
        if agent.suggestions {
            // Query the vector index directly for suggestions
            let query = &event.value;
            let embeddings = self.infra.embedding_service().embed(query).await?;
            let suggestions = self
                .infra
                .vector_index()
                .search(Query::new(embeddings).limit(5u64))
                .await?;

            // Extract just the suggestion strings
            let suggestion_strings = suggestions
                .into_iter()
                .map(|p| p.content.suggestion.clone())
                .collect::<Vec<String>>();

            debug!(suggestions = ?suggestion_strings, "Found suggestions for template rendering");

            // Add suggestions to the event context
            event_context = event_context.suggestions(suggestion_strings);
        }

        debug!(event_context = ?event_context, "Event context");

        // Render the template with the event context
        Ok(self
            .hb
            .render_template(prompt.template.as_str(), &event_context)?)
    }
}
