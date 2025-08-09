use forge_domain::Context;

use crate::utils::get_tool_call;

pub struct Test1 {
    context: Context,
}

impl Test1 {
    pub fn no_incorrect_forge_tag_use(&self) -> bool {
        let message = "<forge_1>test1</forge_1><forge_2>test2</forge_2><forge_3>test3</forge_3>";
        let re = regex::Regex::new(r"<forge_\d+>.*?</forge_\d+>").unwrap();
        let matches: Vec<&str> = re.find_iter(message).map(|m| m.as_str()).collect();
        // matches.iter().all(|value_str| is_tool_call_string(value_str))
        matches.iter().all(|value_str| {
            if let Ok(_) = get_tool_call(value_str) {
                true
            } else {
                false
            }
        })
    }
}
