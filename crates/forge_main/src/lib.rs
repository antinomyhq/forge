pub mod banner;
pub mod acp_runner;
pub mod cli;
mod completer;
mod conversation_selector;
mod display_constants;
mod editor;
mod info;
mod input;
mod model;
mod porcelain;
mod prompt;
mod sandbox;
mod state;
mod stream_renderer;
mod sync_display;
mod title_display;
mod tools_display;
pub mod tracker;
mod ui;
mod utils;
mod vscode;
mod zsh;

mod update;

pub use cli::{Cli, TopLevelCommand};
use lazy_static::lazy_static;
pub use sandbox::Sandbox;
pub use title_display::*;
pub use ui::UI;

lazy_static! {
    pub static ref TRACKER: forge_tracker::Tracker = forge_tracker::Tracker::default();
}
