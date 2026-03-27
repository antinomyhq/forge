//! PTY-based smoke test harness for the forge CLI and ZSH plugin.
//!
//! This crate provides:
//!
//! - [`pty::PtySession`] — a portable pseudo-terminal wrapper for spawning and
//!   driving interactive processes in tests.
//! - [`report`] — ANSI-coloured pass/fail report helpers shared by all smoke
//!   binaries.
//! - [`paths`] — workspace and binary path resolution utilities.

pub mod pty;
pub mod paths;
pub mod report;
