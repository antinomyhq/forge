//! ANSI-coloured pass/fail report helpers for smoke binaries.

// ── colour constants ──────────────────────────────────────────────────────────

pub const GREEN: &str = "\x1b[32m";
pub const RED: &str = "\x1b[31m";
pub const CYAN: &str = "\x1b[36m";
pub const DIM: &str = "\x1b[2m";
pub const BOLD: &str = "\x1b[1m";
pub const RESET: &str = "\x1b[0m";

// ── output helpers ────────────────────────────────────────────────────────────

/// Prints a section header in bold cyan.
pub fn print_header(title: &str) {
    println!("\n{BOLD}{CYAN}══ {title} ══{RESET}");
}

/// Prints each non-empty line of `raw` (after stripping ANSI codes) with a
/// dim `│` gutter prefix.
pub fn print_output(raw: &str) {
    let stripped = strip_ansi(raw);
    for line in stripped.lines() {
        if !line.trim().is_empty() {
            println!("  {DIM}│{RESET} {line}");
        }
    }
}

/// Prints a green ✓ pass line.
pub fn pass(label: &str) {
    println!("{GREEN}  ✓ {label}{RESET}");
}

/// Prints a red ✗ fail line, optionally followed by a reason.
pub fn fail(label: &str, reason: &str) {
    println!("{RED}  ✗ {label}{RESET}");
    if !reason.is_empty() {
        println!("{RED}    {reason}{RESET}");
    }
}

// ── ANSI stripping ────────────────────────────────────────────────────────────

/// Strips ANSI escape sequences from `s` and returns the plain-text result.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Consume characters until the final letter that ends the sequence.
            for ch in chars.by_ref() {
                if ch.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
