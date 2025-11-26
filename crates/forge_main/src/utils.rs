use std::time::Duration;

/// Formats a chrono DateTime as a human-readable relative time string (e.g., "5
/// minutes ago").
pub fn humanize_time(dt: chrono::DateTime<chrono::Utc>) -> String {
    let duration = chrono::Utc::now().signed_duration_since(dt);
    let duration = Duration::from_secs((duration.num_minutes() * 60).max(0) as u64);
    if duration.is_zero() {
        "now".to_string()
    } else {
        format!("{} ago", humantime::format_duration(duration))
    }
}
