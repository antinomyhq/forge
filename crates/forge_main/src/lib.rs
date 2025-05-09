mod banner;
mod cli;
mod completer;
mod editor;
mod info;
mod input;
mod model;
mod prompt;
mod state;
mod tools_display;
mod ui;
mod update;

pub use cli::Cli;
use lazy_static::lazy_static;
pub use ui::UI;
lazy_static! {
    pub static ref TRACKER: forge_tracker::Tracker = forge_tracker::Tracker::default();
}
