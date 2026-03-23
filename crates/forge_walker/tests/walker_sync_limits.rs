use std::fs::{self, File};
use std::io::Write;

use forge_walker::Walker;
use tempfile::tempdir;

#[tokio::test]
async fn max_files_limit_stops_discovery() {
    let dir = tempdir().unwrap();
    let file_limit = 5;
    let total_files = 20;

    for i in 0..total_files {
        File::create(dir.path().join(format!("file{i}.txt")))
            .unwrap()
            .write_all(b"data")
            .unwrap();
    }

    let result = Walker::max_all()
        .cwd(dir.path().to_path_buf())
        .max_files(file_limit)
        .get()
        .await
        .unwrap();

    let file_count = result.iter().filter(|f| !f.is_dir()).count();
    assert!(
        file_count <= file_limit,
        "expected at most {file_limit} files, got {file_count}"
    );
}

#[tokio::test]
async fn max_total_size_limit_stops_discovery() {
    let dir = tempdir().unwrap();
    let size_limit: u64 = 2 * 1024; // 2 KB

    for i in 0..10 {
        let content = vec![b'x'; 1024]; // 1 KB each
        File::create(dir.path().join(format!("file{i}.txt")))
            .unwrap()
            .write_all(&content)
            .unwrap();
    }

    let result = Walker::max_all()
        .cwd(dir.path().to_path_buf())
        .max_total_size(size_limit)
        .get()
        .await
        .unwrap();

    let total_size: u64 = result.iter().filter(|f| !f.is_dir()).map(|f| f.size).sum();
    assert!(
        total_size <= size_limit,
        "expected total size <= {size_limit}, got {total_size}"
    );
}

#[tokio::test]
async fn max_files_zero_returns_only_dirs() {
    let dir = tempdir().unwrap();
    File::create(dir.path().join("a.txt"))
        .unwrap()
        .write_all(b"hi")
        .unwrap();

    let result = Walker::max_all()
        .cwd(dir.path().to_path_buf())
        .max_files(0)
        .get()
        .await
        .unwrap();

    let file_count = result.iter().filter(|f| !f.is_dir()).count();
    assert_eq!(
        file_count, 0,
        "no files should be returned with max_files=0"
    );
}

#[tokio::test]
async fn max_total_size_zero_returns_only_dirs() {
    let dir = tempdir().unwrap();
    File::create(dir.path().join("a.txt"))
        .unwrap()
        .write_all(b"hello")
        .unwrap();

    let result = Walker::max_all()
        .cwd(dir.path().to_path_buf())
        .max_total_size(0)
        .get()
        .await
        .unwrap();

    let total_size: u64 = result.iter().filter(|f| !f.is_dir()).map(|f| f.size).sum();
    assert_eq!(
        total_size, 0,
        "no file bytes should be returned with max_total_size=0"
    );
}

#[tokio::test]
async fn both_limits_together_respect_tighter_constraint() {
    let dir = tempdir().unwrap();

    // 10 files of 1 KB each = 10 KB total
    for i in 0..10 {
        let content = vec![b'a'; 1024];
        File::create(dir.path().join(format!("file{i}.txt")))
            .unwrap()
            .write_all(&content)
            .unwrap();
    }

    // max_files=3 is tighter than max_total_size=50KB
    let result = Walker::max_all()
        .cwd(dir.path().to_path_buf())
        .max_files(3)
        .max_total_size(50 * 1024)
        .get()
        .await
        .unwrap();

    let file_count = result.iter().filter(|f| !f.is_dir()).count();
    assert!(
        file_count <= 3,
        "file count limit should apply: got {file_count}"
    );

    // max_total_size=2KB is tighter than max_files=100
    let result2 = Walker::max_all()
        .cwd(dir.path().to_path_buf())
        .max_files(100)
        .max_total_size(2 * 1024)
        .get()
        .await
        .unwrap();

    let total_size: u64 = result2.iter().filter(|f| !f.is_dir()).map(|f| f.size).sum();
    assert!(
        total_size <= 2 * 1024,
        "total size limit should apply: got {total_size}"
    );
}

#[tokio::test]
async fn nested_directory_files_count_toward_limit() {
    let dir = tempdir().unwrap();
    let nested = dir.path().join("sub").join("deep");
    fs::create_dir_all(&nested).unwrap();

    for i in 0..10 {
        File::create(nested.join(format!("file{i}.txt")))
            .unwrap()
            .write_all(b"content")
            .unwrap();
    }

    let result = Walker::max_all()
        .cwd(dir.path().to_path_buf())
        .max_files(3)
        .get()
        .await
        .unwrap();

    let file_count = result.iter().filter(|f| !f.is_dir()).count();
    assert!(
        file_count <= 3,
        "nested files should count toward limit: got {file_count}"
    );
}
