use std::fmt::Display;

/// Prints an informational message
pub fn info<T: Display>(msg: T) {
    println!("ℹ️ {msg}");
}

/// Prints a success message
pub fn success<T: Display>(msg: T) {
    println!("✅ {msg}");
}

/// Prints an action/step message
pub fn action<T: Display>(msg: T) {
    println!("▶️ {msg}");
}

/// Prints an instruction for the user
pub fn instruction<T: Display>(msg: T) {
    println!("📝 {msg}");
}

/// Prints raw content like a URL or path without prefix
pub fn raw<T: Display>(msg: T) {
    println!("   {msg}");
}

/// Prints a message with high emphasis (for critical information)
pub fn important<T: Display>(msg: T) {
    println!("❗{msg}");
}

/// Prints an error message with detailed error information
pub fn error_details<T: Display, E: Display>(msg: T, err: E) {
    eprintln!("❌ {msg}: {err}");
}
