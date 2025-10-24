pub mod banner;
mod cli;
mod completer;
mod config;
mod conversation_selector;
mod editor;
mod env;
mod info;
mod input;
mod model;
mod porcelain;
mod prompt;
mod sandbox;
mod state;
mod title_display;
mod tools_display;
pub mod tracker;
mod ui;

mod update;

pub use cli::Cli;
pub use config::ConfigManager;
use lazy_static::lazy_static;
pub use sandbox::Sandbox;
pub use title_display::*;
pub use ui::UI;

lazy_static! {
    pub static ref TRACKER: forge_tracker::Tracker = forge_tracker::Tracker::default();
}
