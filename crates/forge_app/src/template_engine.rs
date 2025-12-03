use forge_domain::Template;
use handlebars::{Handlebars, no_escape};
use lazy_static::lazy_static;
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../../templates/"]
struct TemplateSource;

/// Creates a new Handlebars instance with all custom helpers registered.
///
/// This function configures a Handlebars instance with:
/// - The 'inc' helper for incrementing values (useful for 1-based indexing)
/// - Strict mode enabled
/// - No HTML escaping
/// - All embedded templates registered
///
/// This is useful for creating standalone Handlebars instances with consistent
/// configuration across the application.
fn create_handlebar() -> Handlebars<'static> {
    let mut hb = Handlebars::new();
    hb.set_strict_mode(true);
    hb.register_escape_fn(no_escape);

    // Register the 'inc' helper to increment index for 1-based numbering
    hb.register_helper(
        "inc",
        Box::new(
            |h: &handlebars::Helper,
             _: &handlebars::Handlebars,
             _: &handlebars::Context,
             _: &mut handlebars::RenderContext,
             out: &mut dyn handlebars::Output|
             -> handlebars::HelperResult {
                let value = h.param(0).and_then(|v| v.value().as_u64()).ok_or_else(|| {
                    handlebars::RenderErrorReason::ParamNotFoundForIndex("inc", 0)
                })?;
                out.write(&(value + 1).to_string())?;
                Ok(())
            },
        ),
    );

    // Register the 'json' helper to serialize context as JSON string
    hb.register_helper(
        "json",
        Box::new(
            |h: &handlebars::Helper,
             _: &handlebars::Handlebars,
             _: &handlebars::Context,
             _: &mut handlebars::RenderContext,
             out: &mut dyn handlebars::Output|
             -> handlebars::HelperResult {
                let value = h.param(0).ok_or_else(|| {
                    handlebars::RenderErrorReason::ParamNotFoundForIndex("json", 0)
                })?;
                let json_string = serde_json::to_string(value.value())
                    .map_err(|e| handlebars::RenderErrorReason::NestedError(Box::new(e)))?;
                out.write(&json_string)?;
                Ok(())
            },
        ),
    );

    // Register all partial templates
    hb.register_embed_templates::<TemplateSource>().unwrap();

    hb
}

lazy_static! {
    /// Global template engine instance with all custom helpers and templates registered.
    ///
    /// This static instance is lazily initialized on first access and provides:
    /// - The 'inc' helper for incrementing values (useful for 1-based indexing)
    /// - The 'json' helper for serializing values to JSON strings
    /// - Strict mode enabled
    /// - No HTML escaping
    /// - All embedded templates registered
    ///
    /// Use this instance for template rendering throughout the application to avoid
    /// creating multiple Handlebars instances.
    static ref HANDLEBARS: Handlebars<'static> = create_handlebar();
}

/// A wrapper around the Handlebars template engine providing a simplified API.
///
/// This struct provides a clean interface for template rendering using the
/// `Template` type from the domain layer.
pub struct TemplateEngine<'a> {
    handlebar: Handlebars<'a>,
}

impl Default for TemplateEngine<'_> {
    fn default() -> Self {
        Self { handlebar: HANDLEBARS.clone() }
    }
}

impl<'a> TemplateEngine<'a> {
    /// Renders a template with the provided data.
    pub fn render<V: serde::Serialize>(
        &self,
        template: impl Into<Template<V>>,
        data: &V,
    ) -> anyhow::Result<String> {
        let template = template.into();
        Ok(self.handlebar.render(&template.template, data)?)
    }

    /// Renders a template with the provided data.
    pub fn render_template<V: serde::Serialize>(
        &self,
        template: impl Into<Template<V>>,
        data: &V,
    ) -> anyhow::Result<String> {
        let template = template.into();
        Ok(self.handlebar.render_template(&template.template, data)?)
    }

    pub fn handlebar_instance() -> Handlebars<'static> {
        create_handlebar()
    }
}
