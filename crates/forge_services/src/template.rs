use std::collections::HashMap;

use forge_domain::{
    Agent, Compact, Context, Event, EventContext, SystemContext, Template, TemplateService,
};
use handlebars::Handlebars;
use rust_embed::Embed;
use serde_json::Value;


#[derive(Embed)]
#[folder = "../../templates/"]
struct Templates;

#[derive(Clone)]
pub struct ForgeTemplateService {
    hb: Handlebars<'static>,
}

impl Default for ForgeTemplateService {
    fn default() -> Self {
        Self::new()
    }
}

impl ForgeTemplateService {
    pub fn new() -> Self {
        let mut hb = Handlebars::new();
        hb.set_strict_mode(true);
        hb.register_escape_fn(|str| str.to_string());

        // Register all partial templates
        hb.register_embed_templates::<Templates>().unwrap();

        Self { hb }
    }
}

#[async_trait::async_trait]
impl TemplateService for ForgeTemplateService {
    async fn render_system(
        &self,
        _agent: &Agent,
        _prompt: &Template<SystemContext>,
        _variables: &HashMap<String, Value>,
    ) -> anyhow::Result<String> {
        unimplemented!()
    }

    async fn render_event(
        &self,
        _agent: &Agent,
        _prompt: &Template<EventContext>,
        _event: &Event,
        _variables: &HashMap<String, Value>,
    ) -> anyhow::Result<String> {
        unimplemented!()
    }

    async fn render_summarization(
        &self,
        _compaction: &Compact,
        _context: &Context,
    ) -> anyhow::Result<String> {
        unimplemented!()
    }

    fn render(
        &self,
        template: impl ToString,
        object: &impl serde::Serialize,
    ) -> anyhow::Result<String> {
        let template = template.to_string();
        let rendered = self.hb.render(&template, object)?;
        Ok(rendered)
    }
}
