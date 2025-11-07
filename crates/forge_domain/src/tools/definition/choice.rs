use serde::{Deserialize, Serialize};

use crate::ToolName;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub enum ToolChoice {
    #[default]
    None,
    Auto,
    Required,
    Call(ToolName),
}
