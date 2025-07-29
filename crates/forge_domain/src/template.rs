use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(transparent)]
pub struct Template<V> {
    pub template: String,
    _marker: std::marker::PhantomData<V>,
}

impl<T> JsonSchema for Template<T> {
    fn schema_name() -> String {
        String::schema_name()
    }

    fn json_schema(r#gen: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        String::json_schema(r#gen)
    }
}

impl<V> Template<V> {
    pub fn new(template: impl ToString) -> Self {
        Self {
            template: template.to_string(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Get the unique identifier for this template
    pub fn id(&self) -> TemplateId {
        TemplateId::from_template(&self.template)
    }
}

/// A unique identifier for templates based on their content hash
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct TemplateId(u64);

impl TemplateId {
    /// Create a new template id from u64
    pub fn new(n: u64) -> Self {
        Self(n)
    }
    /// Create a new TemplateId from a template string by hashing its content
    pub fn from_template(template: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        template.hash(&mut hasher);
        Self(hasher.finish())
    }

    /// Get the raw hash value
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_template_id_from_string() {
        let template = "Hello {{name}}!";
        let id1 = TemplateId::from_template(template);
        let id2 = TemplateId::from_template(template);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_different_templates_have_different_ids() {
        let template1 = "Hello {{name}}!";
        let template2 = "Goodbye {{name}}!";

        let id1 = TemplateId::from_template(template1);
        let id2 = TemplateId::from_template(template2);

        assert!(id1 != id2);
    }

    #[test]
    fn test_same_template_has_same_id() {
        let template = "Hello {{name}}!";

        let id1 = TemplateId::from_template(template);
        let id2 = TemplateId::from_template(template);

        assert_eq!(id1, id2);
    }

    #[test]
    fn test_template_id_as_u64_returns_hash_value() {
        let template = "Hello {{name}}!";
        let id = TemplateId::from_template(template);

        let hash_value = id.as_u64();
        assert!(hash_value > 0);
    }

    #[test]
    fn test_template_creates_id_automatically() {
        let template_str = "Hello {{name}}!";
        let template = Template::<()>::new(template_str);

        let expected_id = TemplateId::from_template(template_str);
        assert_eq!(template.id(), expected_id);
    }

    #[test]
    fn test_same_template_content_has_same_id() {
        let template1 = Template::<()>::new("Hello {{name}}!");
        let template2 = Template::<()>::new("Hello {{name}}!");

        assert_eq!(template1.id(), template2.id());
    }

    #[test]
    fn test_different_template_content_has_different_id() {
        let template1 = Template::<()>::new("Hello {{name}}!");
        let template2 = Template::<()>::new("Goodbye {{name}}!");

        assert!(template1.id() != template2.id());
    }
}
