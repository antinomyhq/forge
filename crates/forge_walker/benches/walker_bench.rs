use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use forge_walker::Walker;
use tempfile::{TempDir, tempdir};

/// Creates a deeply nested directory structure with 100,000 files
/// This simulates a large monorepo or codebase with realistic nesting
fn create_large_nested_structure() -> TempDir {
    let dir = tempdir().unwrap();
    let base_path = dir.path().to_path_buf();

    println!("Creating 100,000 deeply nested files... (this may take a minute)");

    // Create a tree structure with ~100 directories and ~1000 files each
    // This gives us depth while also having breadth
    let dirs_per_level = 10;
    let levels = 3;
    let files_per_dir = 100;

    // Track total files created
    let mut total_files = 0;

    // Helper to recursively create nested structure
    fn create_nested(
        current: PathBuf,
        level: usize,
        max_level: usize,
        dirs_per_level: usize,
        files_per_dir: usize,
        total_files: &mut usize,
    ) {
        if level > max_level {
            return;
        }

        // Create files in current directory
        for i in 0..files_per_dir {
            let extensions = ["rs", "js", "py", "txt", "md", "json", "toml", "yaml"];
            let ext = extensions[i % extensions.len()];
            let file_path = current.join(format!("file_{}_{}.{}", level, i, ext));

            File::create(file_path)
                .unwrap()
                .write_all(b"// Code content\npub fn example() {\n    println!(\"test\");\n}\n")
                .unwrap();

            *total_files += 1;

            // Progress indicator every 10,000 files
            if (*total_files).is_multiple_of(10000) {
                println!("  Created {} files...", *total_files);
            }
        }

        // Create subdirectories and recurse
        if level < max_level {
            for d in 0..dirs_per_level {
                let subdir = current.join(format!("level{}_dir{}", level + 1, d));
                fs::create_dir(&subdir).unwrap();
                create_nested(
                    subdir,
                    level + 1,
                    max_level,
                    dirs_per_level,
                    files_per_dir,
                    total_files,
                );
            }
        }
    }

    create_nested(
        base_path.clone(),
        0,
        levels,
        dirs_per_level,
        files_per_dir,
        &mut total_files,
    );

    println!("Created {} files total", total_files);
    dir
}

/// Benchmark walking a deeply nested structure with 100,000 files
fn bench_large_nested_codebase(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_nested_codebase");

    // Increase sample size and measurement time for more accurate results on large
    // dataset
    group.sample_size(10);
    group.measurement_time(std::time::Duration::from_secs(30));

    // Create the test structure once (this is expensive)
    println!("\n=== Setting up benchmark ===");
    let test_dir = create_large_nested_structure();
    println!("=== Setup complete ===\n");

    group.bench_function("100k_files_deeply_nested", |b| {
        b.iter(|| {
            let walker = Walker::max_all().cwd(test_dir.path().to_path_buf());
            let result = walker.get_blocking();
            black_box(result)
        });
    });

    group.finish();
}

criterion_group!(benches, bench_large_nested_codebase);
criterion_main!(benches);
