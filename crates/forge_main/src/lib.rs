mod banner;
mod cli;
mod completer;
mod conversation_selector;
mod editor;
mod info;
mod input;
mod model;
mod prompt;
mod sandbox;
mod select;
mod state;
mod title_display;
mod tools_display;
pub mod tracker;
mod ui;
mod update;

pub use cli::Cli;
use lazy_static::lazy_static;
pub use sandbox::Sandbox;
pub use title_display::*;
pub use ui::UI;

lazy_static! {
    pub static ref TRACKER: forge_tracker::Tracker = forge_tracker::Tracker::default();
}
