use serde::{Deserialize, Serialize};
use strum_macros::EnumString;

#[derive(Debug, Clone, Default, EnumString, Serialize, Deserialize)]
#[strum(ascii_case_insensitive)]
pub enum Mode {
    Plan,
    #[default]
    Act,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Plan => write!(f, "PLAN"),
            Mode::Act => write!(f, "ACT"),
        }
    }
}
