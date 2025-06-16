use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use forge_app::{EnvironmentService, TemplateService};
use futures::future;
use handlebars::{no_escape, Handlebars};
use rust_embed::Embed;
use tokio::sync::RwLock;

use crate::{FsReadService, Infrastructure};

#[derive(Embed)]
#[folder = "../../templates/"]
struct Templates;

#[derive(Clone)]
pub struct ForgeTemplateService<F> {
    hb: Arc<RwLock<Handlebars<'static>>>,
    infra: Arc<F>,
}

impl<F: Infrastructure> ForgeTemplateService<F> {
    pub fn new(infra: Arc<F>) -> Self {
        let mut hb = Handlebars::new();
        hb.set_strict_mode(true);
        hb.register_escape_fn(no_escape);

        // Register all partial templates
        hb.register_embed_templates::<Templates>().unwrap();

        Self { hb: Arc::new(RwLock::new(hb)), infra }
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> TemplateService for ForgeTemplateService<F> {
    async fn register_template(&self, path: PathBuf) -> anyhow::Result<()> {
        let cwd = &self.infra.environment_service().get_environment().cwd;

        // Discover and filter unregistered templates in one pass
        let guard = self.hb.read().await;
        let path = if path.is_absolute() {
            path.to_string_lossy().to_string()
        } else {
            cwd.join(path).to_string_lossy().to_string()
        };
        let unregistered_files: Vec<_> = glob::glob(&format!("{path}/*"))?
            .filter_map(|entry| entry.ok())
            .filter(|p| p.is_file())
            .filter(|p| {
                p.file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| guard.get_template(name).is_none())
                    .unwrap_or(true) // Keep files with invalid names for error
                                     // handling
            })
            .collect();
        drop(guard);

        // Read all files concurrently
        let futures = unregistered_files.iter().map(|template_path| async {
            let template_name = template_path
                .file_name()
                .and_then(|name| name.to_str())
                .with_context(|| format!("Invalid filename: {}", template_path.display()))?;
            let template_path = cwd.as_path().join(template_path.clone());
            let content = self
                .infra
                .file_read_service()
                .read_utf8(&template_path)
                .await?;
            Ok::<_, anyhow::Error>((template_name, content))
        });

        let templates = future::join_all(futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;

        // Register all templates if any were found
        if !templates.is_empty() {
            let mut guard = self.hb.write().await;
            for (name, content) in templates {
                let template: handlebars::template::Template = if name.ends_with(".hbs") {
                    handlebars::Template::compile(&content)?
                } else {
                    let mut template = handlebars::template::Template::new();
                    template
                        .elements
                        .push(handlebars::template::TemplateElement::RawString(content));
                    template.name = Some(name.to_owned());
                    template
                };

                guard.register_template(name, template);
            }
        }

        Ok(())
    }

    async fn render(
        &self,
        template: impl ToString + Send,
        object: &(impl serde::Serialize + Sync),
    ) -> anyhow::Result<String> {
        let template = template.to_string();
        let rendered = self.hb.read().await.render_template(&template, object)?;
        Ok(rendered)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;
    use crate::attachment::tests::MockInfrastructure;

    #[tokio::test]
    async fn test_render_simple_template() {
        // Fixture: Create template service and data
        let service = ForgeTemplateService::new(Arc::new(MockInfrastructure::new()));
        let data = json!({
            "name": "Forge",
            "version": "1.0",
            "features": ["templates", "rendering", "handlebars"]
        });

        // Actual: Render a simple template
        let template = "App: {{name}} v{{version}} - Features: {{#each features}}{{this}}{{#unless @last}}, {{/unless}}{{/each}}";
        let actual = service.render(template, &data).await.unwrap();

        // Expected: Result should match the expected string
        let expected = "App: Forge v1.0 - Features: templates, rendering, handlebars";
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_render_partial_system_info() {
        // Fixture: Create template service and data
        let service = ForgeTemplateService::new(Arc::new(MockInfrastructure::new()));
        let data = json!({
            "env": {
                "os": "test-os",
                "cwd": "/test/path",
                "shell": "/bin/test",
                "home": "/home/test"
            },
            "current_time": "2024-01-01 12:00:00 UTC",
            "files": [
                "/file1.txt",
                "/file2.txt"
            ]
        });

        // Actual: Render the partial-system-info template
        let actual = service
            .render("{{> forge-partial-system-info.hbs }}", &data)
            .await
            .unwrap();

        // Expected: Result should contain the rendered system info with substituted
        assert!(actual.contains("<operating_system>test-os</operating_system>"));
    }
}
