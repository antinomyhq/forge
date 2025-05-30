use handlebars::{
    Context, Handlebars, Helper, HelperDef, HelperResult, Output, RenderContext, RenderErrorReason, Renderable,
};
use serde_json::Value as Json;

#[derive(Clone, Copy)]
pub struct ArrayContainsHelper;

impl HelperDef for ArrayContainsHelper {
    fn call<'reg: 'rc, 'rc>(
        &self,
        h: &Helper<'rc>,
        r: &'reg Handlebars<'reg>,
        ctx: &'rc Context,
        rc: &mut RenderContext<'reg, 'rc>,
        out: &mut dyn Output,
    ) -> HelperResult {
        // Get array parameter
        let array = h
            .param(0)
            .ok_or(RenderErrorReason::ParamNotFoundForIndex("array_contains", 0))?;
        
        // Get search value parameter
        let search_value = h
            .param(1)
            .ok_or(RenderErrorReason::ParamNotFoundForIndex("array_contains", 1))?;

        let contains = match array.value() {
            Json::Array(arr) => arr.iter().any(|item| item == search_value.value()),
            _ => false, // Return false if first parameter is not an array
        };

        let tmpl = if contains { h.template() } else { h.inverse() };
        match tmpl {
            Some(t) => t.render(r, ctx, rc, out),
            None => Ok(()),
        }
    }
}

pub static ARRAY_CONTAINS_HELPER: ArrayContainsHelper = ArrayContainsHelper;

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_array_contains() {
        let mut handlebars = Handlebars::new();
        handlebars.register_helper("array_contains", Box::new(ARRAY_CONTAINS_HELPER));

        // Test when value exists in array
        assert_eq!(
            "found".to_string(),
            handlebars
                .render_template(
                    "{{#array_contains arr 'b'}}found{{else}}not found{{/array_contains}}",
                    &json!({"arr": ["a", "b", "c"]})
                )
                .unwrap()
        );

        // Test when value doesn't exist in array
        assert_eq!(
            "not found".to_string(),
            handlebars
                .render_template(
                    "{{#array_contains arr 'd'}}found{{else}}not found{{/array_contains}}",
                    &json!({"arr": ["a", "b", "c"]})
                )
                .unwrap()
        );

        // Test with empty array
        assert_eq!(
            "not found".to_string(),
            handlebars
                .render_template(
                    "{{#array_contains arr 'a'}}found{{else}}not found{{/array_contains}}",
                    &json!({"arr": []})
                )
                .unwrap()
        );

        // Test with non-array value
        assert_eq!(
            "not found".to_string(),
            handlebars
                .render_template(
                    "{{#array_contains notarr 'a'}}found{{else}}not found{{/array_contains}}",
                    &json!({"notarr": "string"})
                )
                .unwrap()
        );

        // Test with number values
        assert_eq!(
            "found".to_string(),
            handlebars
                .render_template(
                    "{{#array_contains nums 42}}found{{else}}not found{{/array_contains}}",
                    &json!({"nums": [1, 42, 100]})
                )
                .unwrap()
        );
    }

    #[test]
    fn test_array_contains_error_cases() {
        let mut handlebars = Handlebars::new();
        handlebars.register_helper("array_contains", Box::new(ARRAY_CONTAINS_HELPER));

        // Test missing first parameter
        assert!(handlebars
            .render_template(
                "{{#array_contains}}found{{else}}not found{{/array_contains}}",
                &json!({})
            )
            .is_err());

        // Test missing second parameter
        assert!(handlebars
            .render_template(
                "{{#array_contains arr}}found{{else}}not found{{/array_contains}}",
                &json!({"arr": [1, 2, 3]})
            )
            .is_err());
    }
}
