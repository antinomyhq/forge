use std::sync::Arc;

use forge_domain::{TemplateService, ToolService};
use handlebars::Handlebars;
use rust_embed::Embed;

use crate::Infrastructure;

#[derive(Embed)]
#[folder = "../../templates/"]
struct Templates;

#[derive(Clone)]
pub struct ForgeTemplateService<F, T> {
    hb: Handlebars<'static>,
    #[allow(dead_code)]
    infra: Arc<F>,
    #[allow(dead_code)]
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
