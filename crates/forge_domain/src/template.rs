use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize, Debug, PartialEq, Eq)]
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
}
