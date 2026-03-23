//! Integration test verifying that workspace file discovery uses bounded
//! walker configuration (the `Walker::sync()` preset) rather than unbounded
//! settings. This prevents runaway memory consumption on large workspaces.
//!
//! Since `FdWalker` is a private module, we test through the public
//! `forge_app::Walker` type to verify the sync preset contract that
//! `FdWalker` depends on.

use std::path::PathBuf;

use forge_app::Walker;

#[test]
fn sync_preset_used_by_fd_walker_is_fully_bounded() {
    // FdWalker::discover() creates Walker::sync().cwd(dir_path).
    // Verify that this preset has all limits set so that a large workspace
    // cannot cause unbounded memory growth.
    let config = Walker::sync();

    assert!(config.max_depth.is_some(), "must bound depth");
    assert!(config.max_breadth.is_some(), "must bound breadth");
    assert!(
        config.max_file_size.is_some(),
        "must bound individual file size"
    );
    assert!(config.max_files.is_some(), "must bound total file count");
    assert!(config.max_total_size.is_some(), "must bound total size");
    assert!(config.skip_binary, "must skip binary files");
}

#[test]
fn sync_preset_cwd_is_overridable() {
    // FdWalker sets cwd to the dir_path argument. Verify the setter works.
    let target = PathBuf::from("/workspace/project");
    let config = Walker::sync().cwd(target.clone());
    assert_eq!(config.cwd, target);
}

#[test]
fn sync_preset_limits_are_reasonable_for_large_monorepos() {
    let config = Walker::sync();

    // Depth of 20 accommodates deeply nested monorepos
    assert!(
        config.max_depth.unwrap() >= 15,
        "depth should be generous for monorepos"
    );

    // At least 10k files for large projects
    assert!(
        config.max_files.unwrap() >= 10_000,
        "file limit should support large projects"
    );

    // At least 100 MB total
    assert!(
        config.max_total_size.unwrap() >= 100 * 1024 * 1024,
        "total size should support large projects"
    );

    // Per-file limit at least 1 MB
    assert!(
        config.max_file_size.unwrap() >= 1024 * 1024,
        "per-file size should allow typical source files"
    );
}

#[test]
fn sync_preset_limits_have_hard_ceiling() {
    let config = Walker::sync();

    // Ensure there IS a ceiling — these should not be astronomically large
    assert!(
        config.max_files.unwrap() <= 1_000_000,
        "file limit should have a reasonable ceiling"
    );
    assert!(
        config.max_total_size.unwrap() <= 10 * 1024 * 1024 * 1024, // 10 GB
        "total size should have a reasonable ceiling"
    );
    assert!(
        config.max_depth.unwrap() <= 100,
        "depth should have a reasonable ceiling"
    );
}
