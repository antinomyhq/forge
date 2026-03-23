use forge_app::Walker;

#[test]
fn sync_preset_has_all_limits_bounded() {
    let sync = Walker::sync();

    assert!(sync.max_depth.is_some(), "sync must bound max_depth");
    assert!(sync.max_breadth.is_some(), "sync must bound max_breadth");
    assert!(
        sync.max_file_size.is_some(),
        "sync must bound max_file_size"
    );
    assert!(sync.max_files.is_some(), "sync must bound max_files");
    assert!(
        sync.max_total_size.is_some(),
        "sync must bound max_total_size"
    );
    assert!(sync.skip_binary, "sync must skip binary files");
}

#[test]
fn sync_preset_is_more_generous_than_conservative() {
    let sync = Walker::sync();
    let conservative = Walker::conservative();

    assert!(
        sync.max_depth.unwrap() > conservative.max_depth.unwrap(),
        "sync depth should exceed conservative"
    );
    assert!(
        sync.max_breadth.unwrap() > conservative.max_breadth.unwrap(),
        "sync breadth should exceed conservative"
    );
    assert!(
        sync.max_files.unwrap() > conservative.max_files.unwrap(),
        "sync max_files should exceed conservative"
    );
    assert!(
        sync.max_total_size.unwrap() > conservative.max_total_size.unwrap(),
        "sync total_size should exceed conservative"
    );
    assert!(
        sync.max_file_size.unwrap() > conservative.max_file_size.unwrap(),
        "sync file_size should exceed conservative"
    );
}

#[test]
fn sync_preset_is_stricter_than_unlimited() {
    let sync = Walker::sync();
    let unlimited = Walker::unlimited();

    // unlimited has no limits (all None), sync has all Some
    assert!(unlimited.max_depth.is_none());
    assert!(sync.max_depth.is_some());

    assert!(unlimited.max_files.is_none());
    assert!(sync.max_files.is_some());

    assert!(unlimited.max_total_size.is_none());
    assert!(sync.max_total_size.is_some());
}

#[test]
fn sync_preset_cwd_defaults_to_empty() {
    let sync = Walker::sync();
    assert!(
        sync.cwd.as_os_str().is_empty(),
        "sync cwd should default to empty path"
    );
}

#[test]
fn sync_preset_cwd_can_be_overridden() {
    let dir = std::path::PathBuf::from("/some/path");
    let sync = Walker::sync().cwd(dir.clone());
    assert_eq!(sync.cwd, dir);
}
