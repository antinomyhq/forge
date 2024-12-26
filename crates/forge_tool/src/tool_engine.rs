use std::collections::HashMap;

use inflector::Inflector;
use schemars::schema::RootSchema;
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Description, FSList, FSRead, FSWrite, ToolTrait};

struct JsonTool<T>(T);

#[async_trait::async_trait]
impl<T: ToolTrait + Sync> ToolTrait for JsonTool<T>
where
    T::Input: serde::de::DeserializeOwned + JsonSchema,
    T::Output: serde::Serialize + JsonSchema,
{
    type Input = Value;
    type Output = Value;

    async fn call(&self, input: Self::Input) -> Result<Self::Output, String> {
        let input: T::Input = serde_json::from_value(input).map_err(|e| e.to_string())?;
        let output: T::Output = self.0.call(input).await?;
        Ok(serde_json::to_value(output).map_err(|e| e.to_string())?)
    }
}

struct ToolDefinition {
    executable: Box<dyn ToolTrait<Input = Value, Output = Value> + Send + Sync + 'static>,
    tool: Tool,
}

pub struct ToolEngine {
    tools: HashMap<ToolName, ToolDefinition>,
}

///
/// Refer to the specification over here:
/// https://glama.ai/blog/2024-11-25-model-context-protocol-quickstart#server
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tool {
    pub name: ToolName,
    pub description: String,
    pub input_schema: RootSchema,
    pub output_schema: Option<RootSchema>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ToolName(String);

impl<A: ToString> From<A> for ToolName {
    fn from(value: A) -> Self {
        ToolName(value.to_string())
    }
}

impl ToolName {
    pub fn into_string(self) -> String {
        self.0
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl ToolEngine {
    pub async fn call(&self, tool_id: &ToolName, input: Value) -> Result<Value, String> {
        match self.tools.get(tool_id) {
            Some(tool) => tool.executable.call(input).await,
            None => Err(format!("No such tool found: {}", tool_id.as_str())),
        }
    }

    pub fn list(&self) -> Vec<Tool> {
        self.tools.values().map(|tool| tool.tool.clone()).collect()
    }

    fn import<T>(tool: T) -> (ToolName, ToolDefinition)
    where
        T: ToolTrait + Description + Send + Sync + 'static,
        T::Input: serde::de::DeserializeOwned + JsonSchema,
        T::Output: serde::Serialize + JsonSchema,
    {
        let id = std::any::type_name::<T>()
            .split("::")
            .last()
            .unwrap()
            .to_snake_case();
        let executable = Box::new(JsonTool(tool));
        let tool = Tool {
            name: ToolName(id.clone()),
            description: T::description().to_string(),
            input_schema: schema_for!(T::Input),
            output_schema: Some(schema_for!(T::Output)),
        };

        (ToolName(id), ToolDefinition { executable, tool })
    }
}

impl Default for ToolEngine {
    fn default() -> Self {
        let tools: HashMap<ToolName, ToolDefinition> = HashMap::from([
            ToolEngine::import(FSRead),
            ToolEngine::import(FSWrite),
            ToolEngine::import(FSList),
            // TODO: uncomment them later on, as of now we only need the above tools.
            // ToolEngine::import(FSSearch),
            // ToolEngine::import(FSFileInfo),
            // ToolEngine::import(FSReplace),
            // ToolEngine::import(Outline),
            // ToolEngine::import(Think::default()),
            // ToolEngine::import(Shell::default()),
            // ToolEngine::import(AskFollowUpQuestion),
        ]);

        Self { tools }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::think::Think;
    use crate::{FSFileInfo, FSSearch};

    #[test]
    fn test_id() {
        assert!(ToolEngine::import(FSRead)
            .0
            .into_string()
            .ends_with("fs_read"));
        assert!(ToolEngine::import(FSSearch)
            .0
            .into_string()
            .ends_with("fs_search"));
        assert!(ToolEngine::import(FSList)
            .0
            .into_string()
            .ends_with("fs_list"));
        assert!(ToolEngine::import(FSFileInfo)
            .0
            .into_string()
            .ends_with("file_info"));
        assert!(ToolEngine::import(Think::default())
            .0
            .into_string()
            .ends_with("think"));
    }
}
