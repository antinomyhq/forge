use forge_domain::Context;

use crate::utils::{get_tool_call, is_tool_completion_call};

pub struct Test2 {
    context: Context,
}

impl Test2 {
    pub fn has_tool_completion_call(&self) -> bool {
        // Figure out which message will be the correct one to chekc out
        let message = self.context.messages.first();
        if let Some(_msg) = message
            && let Ok(tool_call) = get_tool_call("")
        {
            return is_tool_completion_call(&tool_call);
        }
        false
    }
}
