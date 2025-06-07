use handlebars::Handlebars;
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../../templates/"]
pub struct Templates;

impl Templates {
    /// Render templates without service dependency
    pub fn render(template: &str, object: &impl serde::Serialize) -> anyhow::Result<String> {
        // Create handlebars instance with same configuration as ForgeTemplateService
        let mut hb = Handlebars::new();
        hb.set_strict_mode(true);
        hb.register_escape_fn(|str| str.to_string());

        // Register all partial templates
        hb.register_embed_templates::<Templates>()?;

        // Render the template
        let rendered = hb.render_template(template, object)?;
        Ok(rendered)
    }
}
