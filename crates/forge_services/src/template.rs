use forge_domain::TemplateService;
use handlebars::Handlebars;
use rust_embed::Embed;

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
