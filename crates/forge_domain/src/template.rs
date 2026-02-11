use std::borrow::Cow;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Default, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(transparent)]
pub struct Template<V> {
    pub template: String,
    _marker: std::marker::PhantomData<V>,
}

impl<T> JsonSchema for Template<T> {
    fn schema_name() -> Cow<'static, str> {
        String::schema_name()
    }

    fn json_schema(r#gen: &mut schemars::generate::SchemaGenerator) -> schemars::Schema {
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

impl<S: AsRef<str>> From<S> for Template<Value> {
    fn from(value: S) -> Self {
        Template::new(value.as_ref())
    }
}
