use std::sync::Arc;

use forge_domain::TemplateService;
use handlebars::Handlebars;
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../../templates/"]
struct Templates;

#[derive(Clone)]
pub struct ForgeTemplateService {
    hb: Arc<Handlebars<'static>>,
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

        Self { hb: Arc::new(hb) }
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
        let rendered = self.hb.render_template(&template, object)?;
        Ok(rendered)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_render_simple_template() {
        // Fixture: Create template service and data
        let service = ForgeTemplateService::new();
        let data = json!({
            "name": "Forge",
            "version": "1.0",
            "features": ["templates", "rendering", "handlebars"]
        });

        // Actual: Render a simple template
        let template = "App: {{name}} v{{version}} - Features: {{#each features}}{{this}}{{#unless @last}}, {{/unless}}{{/each}}";
        let actual = service.render(template, &data).unwrap();

        // Expected: Result should match the expected string
        let expected = "App: Forge v1.0 - Features: templates, rendering, handlebars";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_partial_system_info() {
        // Fixture: Create template service and data
        let service = ForgeTemplateService::new();
        let data = json!({
            "env": {
                "os": "test-os",
                "cwd": "/test/path",
                "shell": "/bin/test",
                "home": "/home/test"
            },
            "files": [
                "/file1.txt",
                "/file2.txt"
            ]
        });

        // Actual: Render the partial-system-info template
        let actual = service
            .render("{{> partial-system-info.hbs }}", &data)
            .unwrap();

        // Expected: Result should contain the rendered system info with substituted
        // values
        assert!(actual.contains("<operating_system>test-os</operating_system>"));
    }
}
