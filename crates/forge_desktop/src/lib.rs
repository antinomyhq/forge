// Re-exports the Tauri app functionality
// This allows the forge_desktop crate to be used as a normal crate
// within the workspace

// Re-export the run function that starts the Tauri app
pub fn run() {
    forge_desktop_lib::run();
}