mod banner;
mod cli;
mod completer;
mod editor;
mod info;
mod input;
mod model;
mod prompt;
mod select;
mod state;
mod tools_display;
pub mod tracker;
mod ui;
mod update;

pub use cli::Cli;
use once_cell::sync::OnceCell;
pub use ui::UI;

pub static TRACKER: OnceCell<forge_tracker::Tracker> = OnceCell::new();