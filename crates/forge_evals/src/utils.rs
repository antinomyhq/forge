use forge_domain::{ToolCallFull, ToolName};

pub(crate) fn get_tool_call(value_str: &str) -> anyhow::Result<ToolCallFull> {
    let cleaned = value_str.replace('\n', "");
    const START_TAG: &str = "<forge_tool_call>";
    const END_TAG: &str = "</forge_tool_call>";
    if cleaned.starts_with(START_TAG) && cleaned.ends_with(END_TAG) {
        let json_str = &cleaned[START_TAG.len()..cleaned.len() - END_TAG.len()];
        if let Ok(tool_call) = serde_json::from_str::<ToolCallFull>(json_str) {
            return Ok(tool_call);
        }
    }
    Err(anyhow::anyhow!("Invalid tool call format"))
}

pub(crate) fn is_tool_completion_call(tool_call: &ToolCallFull) -> bool {
    tool_call.name == ToolName::from("tool_call_completion")
}
