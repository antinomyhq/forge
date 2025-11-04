use handlebars::{Handlebars, no_escape};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../../templates/"]
pub struct Templates;

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
pub fn create_handlebars() -> Handlebars<'static> {
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

    // Register all partial templates
    hb.register_embed_templates::<Templates>().unwrap();

    hb
}
