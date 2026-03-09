use forge_domain::Transformer;

use crate::dto::anthropic::Request;

/// Transformer that removes the output_format field from Anthropic requests.
///
/// This is specifically needed for Vertex AI Anthropic integration, which does
/// not support the `output_format` parameter. When present, it causes a 400
/// error: "output_format: Extra inputs are not permitted"
///
/// # Example
///
/// Before transformation:
/// ```json
/// {
///   "messages": [...],
///   "output_format": {
///     "type": "json_schema",
///     "schema": {...}
///   }
/// }
/// ```
///
/// After transformation:
/// ```json
/// {
///   "messages": [...]
/// }
/// ```
pub struct RemoveOutputFormat;

impl Transformer for RemoveOutputFormat {
    type Value = Request;

    fn transform(&mut self, mut request: Self::Value) -> Self::Value {
        request.output_format = None;
        request
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use schemars::JsonSchema;
    use serde::Deserialize;

    use super::*;
    use crate::dto::anthropic::OutputFormat;

    #[derive(Deserialize, JsonSchema)]
    #[schemars(title = "test_response")]
    #[allow(dead_code)]
    struct TestResponse {
        name: String,
    }

    #[test]
    fn test_removes_output_format() {
        let schema = schemars::schema_for!(TestResponse);
        let fixture = Request::default().output_format(OutputFormat::JsonSchema { schema });

        let actual = RemoveOutputFormat.transform(fixture);

        assert_eq!(actual.output_format, None);
    }

    #[test]
    fn test_preserves_none_output_format() {
        let fixture = Request::default();

        let actual = RemoveOutputFormat.transform(fixture);

        assert_eq!(actual.output_format, None);
    }
}
