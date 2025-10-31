// Environment variable names
pub const FORGE_SHOW_TASK_STATS: &str = "FORGE_SHOW_TASK_STATS";

/// Check if the completion prompt should be shown
///
/// Returns true if the environment variable is not set, cannot be parsed, or is
/// set to "true" (case-insensitive). Returns false only if explicitly set to
/// "false".
pub fn should_show_completion_prompt() -> bool {
    std::env::var(FORGE_SHOW_TASK_STATS)
        .ok()
        .and_then(|val| val.trim().parse::<bool>().ok())
        .unwrap_or(true)
}
