use std::path::Path;

use crate::{Match, MatchResult};

/// Formats a path for display, converting absolute paths to relative when
/// possible
///
/// If the path starts with the current working directory, returns a
/// relative path. Otherwise, returns the original absolute path.
///
/// # Arguments
/// * `path` - The path to format
/// * `cwd` - The current working directory path
///
/// # Returns
/// * A formatted path string
pub fn format_display_path(path: &Path, cwd: &Path) -> String {
    // Try to create a relative path for display if possible
    let display_path = if path.starts_with(cwd) {
        match path.strip_prefix(cwd) {
            Ok(rel_path) => rel_path.display().to_string(),
            Err(_) => path.display().to_string(),
        }
    } else {
        path.display().to_string()
    };

    if display_path.is_empty() {
        ".".to_string()
    } else {
        display_path
    }
}

/// Truncates a key string for display purposes
///
/// If the key length is 20 characters or less, returns it unchanged.
/// Otherwise, shows the first 13 characters and last 4 characters with "..." in
/// between.
///
/// # Arguments
/// * `key` - The key string to truncate
///
/// # Returns
/// * A truncated version of the key for safe display
pub use forge_domain::truncate_key;

pub fn format_match(matched: &Match, base_dir: &Path) -> String {
    match &matched.result {
        Some(MatchResult::Error(err)) => format!("Error reading {}: {}", matched.path, err),
        Some(MatchResult::Found { line_number, line }) => {
            let path = format_display_path(Path::new(&matched.path), base_dir);
            match line_number {
                Some(num) => format!("{}:{}:{}", path, num, line),
                None => format!("{}:{}", path, line),
            }
        }
        Some(MatchResult::Count { count }) => {
            format!(
                "{}:{}",
                format_display_path(Path::new(&matched.path), base_dir),
                count
            )
        }
        Some(MatchResult::FileMatch) => format_display_path(Path::new(&matched.path), base_dir),
        Some(MatchResult::ContextMatch { line_number, line, before_context, after_context }) => {
            let path = format_display_path(Path::new(&matched.path), base_dir);
            let mut output = String::new();

            // Add before context lines
            for ctx_line in before_context {
                output.push_str(&format!("{}-{}\n", path, ctx_line));
            }

            // Add the match line
            match line_number {
                Some(num) => output.push_str(&format!("{}:{}:{}", path, num, line)),
                None => output.push_str(&format!("{}:{}", path, line)),
            }

            // Add after context lines
            for ctx_line in after_context {
                output.push_str(&format!("\n{}-{}", path, ctx_line));
            }

            output
        }
        None => format_display_path(Path::new(&matched.path), base_dir),
    }
}

/// Computes SHA-256 hash of the given content
///
/// General-purpose utility function that computes a SHA-256 hash of string
/// content. Returns a consistent hexadecimal representation that can be used
/// for content comparison, caching, or change detection.
///
/// # Arguments
/// * `content` - The content string to hash
///
/// # Returns
/// * A hexadecimal string representation of the SHA-256 hash
pub fn compute_hash(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Normalizes a JSON schema to meet LLM provider requirements
///
/// Many LLM providers (OpenAI, Anthropic) require that all object types in JSON
/// schemas explicitly set `additionalProperties: false`. This function
/// recursively processes the schema to add this requirement.
///
/// Additionally, for OpenAI compatibility, it ensures:
/// - All objects have a `properties` field (even if empty)
/// - All objects have a `required` array with all property keys
///
/// # Arguments
/// * `schema` - The JSON schema to normalize (will be modified in place)
/// * `strict_mode` - If true, adds `properties` and `required` fields for
///   OpenAI compatibility
///
/// # Example
///
/// ```rust,ignore
/// use serde_json::json;
/// use forge_app::utils::normalize_json_schema;
///
/// let mut schema = json!({
///     "type": "object",
///     "properties": {
///         "name": { "type": "string" }
///     }
/// });
///
/// normalize_json_schema(&mut schema, false);
///
/// assert_eq!(schema["additionalProperties"], json!(false));
/// ```
pub fn enforce_strict_schema(schema: &mut serde_json::Value, strict_mode: bool) {
    match schema {
        serde_json::Value::Object(map) => {
            // Check if this is an object type
            let is_object = map
                .get("type")
                .and_then(|value| value.as_str())
                .is_some_and(|ty| ty == "object")
                || map.contains_key("properties");

            if is_object {
                // OpenAI strict mode: ensure properties field exists
                if strict_mode && !map.contains_key("properties") {
                    map.insert(
                        "properties".to_string(),
                        serde_json::Value::Object(serde_json::Map::new()),
                    );
                }

                // Both OpenAI and Anthropic require this field to be `false` for objects
                map.insert(
                    "additionalProperties".to_string(),
                    serde_json::Value::Bool(false),
                );

                // OpenAI strict mode: ensure required field exists with all property keys
                if strict_mode {
                    let required_keys = map
                        .get("properties")
                        .and_then(|value| value.as_object())
                        .map(|props| {
                            let mut keys = props.keys().cloned().collect::<Vec<_>>();
                            keys.sort();
                            keys
                        })
                        .unwrap_or_default();

                    let required_values = required_keys
                        .into_iter()
                        .map(serde_json::Value::String)
                        .collect::<Vec<_>>();

                    map.insert(
                        "required".to_string(),
                        serde_json::Value::Array(required_values),
                    );
                }
            }

            // Recursively normalize nested schemas
            for value in map.values_mut() {
                enforce_strict_schema(value, strict_mode);
            }
        }
        serde_json::Value::Array(items) => {
            for value in items {
                enforce_strict_schema(value, strict_mode);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_normalize_json_schema_anthropic_mode() {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            }
        });

        enforce_strict_schema(&mut schema, false);

        assert_eq!(schema["additionalProperties"], json!(false));
        // In non-strict mode, required field is not added
        assert_eq!(schema.get("required"), None);
    }

    #[test]
    fn test_normalize_json_schema_openai_strict_mode() {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "age": { "type": "number" }
            }
        });

        enforce_strict_schema(&mut schema, true);

        assert_eq!(schema["additionalProperties"], json!(false));
        assert_eq!(schema["required"], json!(["age", "name"]));
    }

    #[test]
    fn test_normalize_json_schema_adds_empty_properties_in_strict_mode() {
        let mut schema = json!({
            "type": "object"
        });

        enforce_strict_schema(&mut schema, true);

        assert_eq!(schema["properties"], json!({}));
        assert_eq!(schema["additionalProperties"], json!(false));
        assert_eq!(schema["required"], json!([]));
    }

    #[test]
    fn test_normalize_json_schema_nested_objects() {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "user": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" }
                    }
                }
            }
        });

        enforce_strict_schema(&mut schema, false);

        assert_eq!(schema["additionalProperties"], json!(false));
        assert_eq!(
            schema["properties"]["user"]["additionalProperties"],
            json!(false)
        );
    }
}
