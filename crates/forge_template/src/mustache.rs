use std::collections::HashMap;

use colored::Colorize;
use handlebars::{Context, Handlebars, Helper, HelperResult, Output, RenderContext, RenderError};
use serde_json::Value;

/// A template engine that supports Mustache/Handlebars templates with custom
/// color helpers.
///
/// # Features
/// - Full Handlebars/Mustache template syntax support
/// - Custom color helpers (red, green, yellow, blue, etc.)
/// - Conditional rendering based on variables
/// - Can be used with or without color support
///
/// # Examples
/// ```
/// use forge_template::MustacheTemplateEngine;
/// use std::collections::HashMap;
///
/// let mut engine = MustacheTemplateEngine::new(true);
/// let mut data = HashMap::new();
/// data.insert("name".to_string(), "Alice".to_string());
/// data.insert("level".to_string(), "info".to_string());
///
/// let template = "{{#if (eq level \"info\")}}{{white name}}{{/if}}";
/// let result = engine.render(template, &data).unwrap();
/// ```
pub struct MustacheTemplateEngine {
    handlebars: Handlebars<'static>,
}

impl MustacheTemplateEngine {
    /// Creates a new template engine
    ///
    /// # Arguments
    /// * `with_colors` - If true, color helpers will output ANSI color codes
    pub fn new(with_colors: bool) -> Self {
        let mut handlebars = Handlebars::new();
        handlebars.set_strict_mode(false);

        // Register color helpers
        Self::register_color_helpers(&mut handlebars, with_colors);

        // Register utility helpers
        Self::register_utility_helpers(&mut handlebars);

        Self { handlebars }
    }

    /// Renders a template with the provided data
    ///
    /// # Arguments
    /// * `template` - The Handlebars/Mustache template string
    /// * `data` - A map of variable names to their values
    ///
    /// # Errors
    /// Returns an error if the template is invalid or rendering fails
    pub fn render(
        &mut self,
        template: &str,
        data: &HashMap<String, String>,
    ) -> Result<String, RenderError> {
        // Convert HashMap to JSON Value
        let json_data: serde_json::Map<String, Value> = data
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect();

        self.handlebars
            .render_template(template, &Value::Object(json_data))
    }

    /// Registers color helper functions
    fn register_color_helpers(handlebars: &mut Handlebars<'static>, with_colors: bool) {
        // Macro to register color helpers
        macro_rules! register_color {
            ($name:expr, $color_fn:expr) => {
                handlebars.register_helper(
                    $name,
                    Box::new(
                        move |h: &Helper,
                              _: &Handlebars,
                              _: &Context,
                              _: &mut RenderContext,
                              out: &mut dyn Output|
                              -> HelperResult {
                            let text =
                                h.param(0).and_then(|v| v.value().as_str()).ok_or_else(|| {
                                    RenderError::strict_error(Some(&format!(
                                        "Color helper '{}' requires a string parameter",
                                        $name
                                    )))
                                })?;

                            let output = if with_colors {
                                $color_fn(text)
                            } else {
                                text.to_string()
                            };

                            out.write(&output)?;
                            Ok(())
                        },
                    ),
                );
            };
        }

        // Register basic color helpers
        register_color!("red", |s: &str| s.red().to_string());
        register_color!("green", |s: &str| s.green().to_string());
        register_color!("yellow", |s: &str| s.yellow().to_string());
        register_color!("blue", |s: &str| s.blue().to_string());
        register_color!("magenta", |s: &str| s.magenta().to_string());
        register_color!("cyan", |s: &str| s.cyan().to_string());
        register_color!("white", |s: &str| s.white().to_string());
        register_color!("black", |s: &str| s.black().to_string());

        // Register bright color helpers
        register_color!("bright_red", |s: &str| s.bright_red().to_string());
        register_color!("bright_green", |s: &str| s.bright_green().to_string());
        register_color!("bright_yellow", |s: &str| s.bright_yellow().to_string());
        register_color!("bright_blue", |s: &str| s.bright_blue().to_string());
        register_color!("bright_magenta", |s: &str| s.bright_magenta().to_string());
        register_color!("bright_cyan", |s: &str| s.bright_cyan().to_string());
        register_color!("bright_white", |s: &str| s.bright_white().to_string());
        register_color!("bright_black", |s: &str| s.bright_black().to_string());

        // Register style helpers
        register_color!("bold", |s: &str| s.bold().to_string());
        register_color!("dimmed", |s: &str| s.dimmed().to_string());
        register_color!("italic", |s: &str| s.italic().to_string());
        register_color!("underline", |s: &str| s.underline().to_string());
    }

    /// Registers utility helper functions
    fn register_utility_helpers(handlebars: &mut Handlebars<'static>) {
        // Register 'eq' helper for equality comparison
        handlebars.register_helper(
            "eq",
            Box::new(
                |h: &Helper,
                 _: &Handlebars,
                 _: &Context,
                 _: &mut RenderContext,
                 out: &mut dyn Output|
                 -> HelperResult {
                    let param1 = h.param(0).and_then(|v| v.value().as_str());
                    let param2 = h.param(1).and_then(|v| v.value().as_str());

                    let result = match (param1, param2) {
                        (Some(a), Some(b)) => a == b,
                        _ => false,
                    };

                    out.write(if result { "true" } else { "" })?;
                    Ok(())
                },
            ),
        );

        // Register 'ne' helper for inequality comparison
        handlebars.register_helper(
            "ne",
            Box::new(
                |h: &Helper,
                 _: &Handlebars,
                 _: &Context,
                 _: &mut RenderContext,
                 out: &mut dyn Output|
                 -> HelperResult {
                    let param1 = h.param(0).and_then(|v| v.value().as_str());
                    let param2 = h.param(1).and_then(|v| v.value().as_str());

                    let result = match (param1, param2) {
                        (Some(a), Some(b)) => a != b,
                        _ => true,
                    };

                    out.write(if result { "true" } else { "" })?;
                    Ok(())
                },
            ),
        );

        // Register 'is_not_empty' helper to check if a string is not empty
        handlebars.register_helper(
            "is_not_empty",
            Box::new(
                |h: &Helper,
                 _: &Handlebars,
                 _: &Context,
                 _: &mut RenderContext,
                 out: &mut dyn Output|
                 -> HelperResult {
                    let param = h.param(0).and_then(|v| v.value().as_str());

                    let result = match param {
                        Some(s) => !s.is_empty(),
                        None => false,
                    };

                    out.write(if result { "true" } else { "" })?;
                    Ok(())
                },
            ),
        );
    }
}

impl Default for MustacheTemplateEngine {
    fn default() -> Self {
        Self::new(true)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn fixture_data() -> HashMap<String, String> {
        let mut data = HashMap::new();
        data.insert("name".to_string(), "Alice".to_string());
        data.insert("level".to_string(), "info".to_string());
        data.insert("message".to_string(), "Hello World".to_string());
        data
    }

    #[test]
    fn test_simple_variable_substitution() {
        let mut engine = MustacheTemplateEngine::new(false);
        let data = fixture_data();

        let template = "Hello {{name}}!";
        let actual = engine.render(template, &data).unwrap();
        let expected = "Hello Alice!";

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_color_helper_without_colors() {
        let mut engine = MustacheTemplateEngine::new(false);
        let data = fixture_data();

        let template = "{{red message}}";
        let actual = engine.render(template, &data).unwrap();
        let expected = "Hello World";

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_color_helper_with_colors() {
        let mut engine = MustacheTemplateEngine::new(true);
        let data = fixture_data();

        let template = "{{red message}}";
        let actual = engine.render(template, &data).unwrap();
        let expected = "Hello World".red().to_string();

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_multiple_color_helpers() {
        let mut engine = MustacheTemplateEngine::new(true);
        let data = fixture_data();

        let template = "{{red name}} says {{green message}}";
        let actual = engine.render(template, &data).unwrap();
        let expected = format!("{} says {}", "Alice".red(), "Hello World".green());

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_conditional_with_eq_helper() {
        let mut engine = MustacheTemplateEngine::new(false);
        let data = fixture_data();

        let template = "{{#if (eq level \"info\")}}Info level{{/if}}";
        let actual = engine.render(template, &data).unwrap();
        let expected = "Info level";

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_conditional_with_eq_helper_false() {
        let mut engine = MustacheTemplateEngine::new(false);
        let data = fixture_data();

        let template = "{{#if (eq level \"error\")}}Error level{{/if}}";
        let actual = engine.render(template, &data).unwrap();
        let expected = "";

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_conditional_with_color() {
        let mut engine = MustacheTemplateEngine::new(true);
        let data = fixture_data();

        let template = "{{#if (eq level \"info\")}}{{white message}}{{else}}{{red message}}{{/if}}";
        let actual = engine.render(template, &data).unwrap();
        let expected = "Hello World".white().to_string();

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_bold_helper() {
        let mut engine = MustacheTemplateEngine::new(true);
        let data = fixture_data();

        let template = "{{bold name}}";
        let actual = engine.render(template, &data).unwrap();
        let expected = "Alice".bold().to_string();

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_dimmed_helper() {
        let mut engine = MustacheTemplateEngine::new(true);
        let data = fixture_data();

        let template = "{{dimmed message}}";
        let actual = engine.render(template, &data).unwrap();
        let expected = "Hello World".dimmed().to_string();

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_bright_colors() {
        let mut engine = MustacheTemplateEngine::new(true);
        let data = fixture_data();

        let template = "{{bright_yellow message}}";
        let actual = engine.render(template, &data).unwrap();
        let expected = "Hello World".bright_yellow().to_string();

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_complex_template() {
        let mut engine = MustacheTemplateEngine::new(true);
        let mut data = HashMap::new();
        data.insert("timestamp".to_string(), "12:34:56".to_string());
        data.insert("input".to_string(), "100".to_string());
        data.insert("output".to_string(), "200".to_string());
        data.insert("cost".to_string(), "$0.0023".to_string());
        data.insert("cache_pct".to_string(), "50%".to_string());
        data.insert("title".to_string(), "Task Complete".to_string());
        data.insert("subtitle".to_string(), "(success)".to_string());
        data.insert("level".to_string(), "info".to_string());

        let template = "{{#if (eq level \"info\")}}{{dimmed \"[\"}}{{white timestamp}} {{white input}}/{{white output}} {{white cost}} {{white cache_pct}}{{dimmed \"]\"}} {{bold title}} {{dimmed subtitle}}{{/if}}";
        let actual = engine.render(template, &data).unwrap();

        // We can't easily test the exact colored output, but we can verify it contains
        // key text
        assert!(actual.contains("12:34:56"));
        assert!(actual.contains("100"));
        assert!(actual.contains("200"));
        assert!(actual.contains("Task Complete"));
    }

    #[test]
    fn test_ne_helper() {
        let mut engine = MustacheTemplateEngine::new(false);
        let data = fixture_data();

        let template = "{{#if (ne level \"error\")}}Not an error{{/if}}";
        let actual = engine.render(template, &data).unwrap();
        let expected = "Not an error";

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_missing_variable() {
        let mut engine = MustacheTemplateEngine::new(false);
        let data = HashMap::new();

        let template = "Hello {{name}}!";
        let actual = engine.render(template, &data).unwrap();
        let expected = "Hello !";

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_is_not_empty_helper_with_value() {
        let mut engine = MustacheTemplateEngine::new(false);
        let mut data = HashMap::new();
        data.insert("name".to_string(), "Alice".to_string());

        let template = "{{#if (is_not_empty name)}}Hello {{name}}!{{/if}}";
        let actual = engine.render(template, &data).unwrap();
        let expected = "Hello Alice!";

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_is_not_empty_helper_with_empty_string() {
        let mut engine = MustacheTemplateEngine::new(false);
        let mut data = HashMap::new();
        data.insert("name".to_string(), String::new());

        let template = "{{#if (is_not_empty name)}}Hello {{name}}!{{else}}No name{{/if}}";
        let actual = engine.render(template, &data).unwrap();
        let expected = "No name";

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_is_not_empty_helper_with_missing_variable() {
        let mut engine = MustacheTemplateEngine::new(false);
        let data = HashMap::new();

        let template = "{{#if (is_not_empty name)}}Hello {{name}}!{{else}}No name{{/if}}";
        let actual = engine.render(template, &data).unwrap();
        let expected = "No name";

        assert_eq!(actual, expected);
    }
}
