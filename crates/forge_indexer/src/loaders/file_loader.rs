use std::{path::PathBuf, task::Poll};

use derive_setters::Setters;

use crate::loaders::Loader;

#[derive(Debug, Clone, Setters)]
pub struct FileLoader {
    pub(crate) path: PathBuf,
    pub(crate) ext: Option<Vec<String>>,
}

impl FileLoader {
    // Accept any type that can be referenced as a Path (e.g. &str, &Path, PathBuf)
    pub fn new<P: AsRef<std::path::Path>>(path: P, ext: Vec<String>) -> Self {
        Self { path: path.as_ref().to_path_buf(), ext: Some(ext) }
    }
}

impl Loader for FileLoader {
    async fn load(&self) -> anyhow::Result<Vec<super::Node>> {
        let walker = forge_walker::Walker::max_all()
            .cwd(self.path.clone())
            .skip_binary(true);

        // Get the list of files
        let mut files = walker.get().await?;

        // Filter by extension
        if let Some(ext) = &self.ext {
            files.retain(|node| {
                // Build the full path relative to configured cwd so extension checks are
                // correct
                let full_path = self.path.join(&node.path);
                if let Some(file_ext) = full_path.extension() {
                    return ext.contains(&file_ext.to_string_lossy().to_string());
                }
                false
            });
        }

        // Read file contents
        let mut nodes = Vec::with_capacity(files.len());
        for node in files {
            // Build absolute path to the file by joining with configured cwd
            let full_path = self.path.join(&node.path);
            // Skip directories or non-files returned by the walker
            if !full_path.is_file() {
                continue;
            }
            let content = std::fs::read_to_string(&full_path)?;
            nodes.push(super::Node { path: full_path, content });
        }

        Ok(nodes)
    }
}

impl futures::Stream for FileLoader {
    type Item = Vec<PathBuf>;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        // Only emit once, then end the stream
        if self.as_ref().ext.is_some() {
            let walker = forge_walker::Walker::max_all()
                .cwd(self.path.clone())
                .skip_binary(true);

            // Get the list of files
            let mut files = walker.get_blocking().unwrap();

            // Filter by extension
            if let Some(ext) = &self.ext {
                files.retain(|node| {
                    // Build the full path relative to configured cwd so extension checks are correct
                    let full_path = self.path.join(&node.path);
                    if let Some(file_ext) = full_path.extension() {
                        return ext.contains(&file_ext.to_string_lossy().to_string());
                    }
                    false
                });
            }
            
            // Mark as consumed
            self.as_mut().ext = None;

            Poll::Ready(Some(
                files.into_iter()
                    .map(|f| self.path.join(f.path))
                    .filter(|path| path.is_file())
                    .collect(),
            ))
        } else {
            // Stream is exhausted
            Poll::Ready(None)
        }
    }
}

// #[cfg(test)]
// mod tests {
//     use pretty_assertions::assert_eq;
//     use tempfile::tempdir;
//     use std::fs::write;
//     use crate::loaders::Loader;

//     // Reference: FileLoader implementation at src/loaders/file_loader.rs:1
//     // Reference: Node and Loader definitions at src/loaders/mod.rs:1

//     #[tokio::test]
//     async fn test_file_loader_loads_all_files() {
//         // fixture: create a temporary directory with two files
//         let dir = tempdir().unwrap();
//         let fixture = dir.path().to_path_buf();

//         write(fixture.join("a.txt"), "hello").unwrap();
//         write(fixture.join("b.md"), "world").unwrap();

//         let loader = super::FileLoader::new(fixture.clone());

//         // actual: load files using the FileLoader
//         let actual = loader.load().await.unwrap();

//         // extract file names and contents for assertion
//         let mut names: Vec<String> = actual.iter().map(|n|
// n.path.file_name().unwrap().to_string_lossy().to_string()).collect();
//         names.sort();

//         let mut contents: Vec<String> = actual.iter().map(|n|
// n.content.clone()).collect();         contents.sort();

//         // expected values
//         let expected_names = vec!["a.txt".to_string(), "b.md".to_string()];
//         let mut expected_contents = vec!["hello".to_string(),
// "world".to_string()];         expected_contents.sort();

//         assert_eq!(names, expected_names);
//         assert_eq!(contents, expected_contents);
//     }

//     #[tokio::test]
//     async fn test_file_loader_filters_by_extension() {
//         // fixture: create a temporary directory with three files
//         let dir = tempdir().unwrap();
//         let fixture = dir.path().to_path_buf();

//         write(fixture.join("a.txt"), "one").unwrap();
//         write(fixture.join("b.md"), "two").unwrap();
//         write(fixture.join("c.txt"), "three").unwrap();

//         // only load .txt files
//         let loader =
// super::FileLoader::new(fixture.clone()).ext(Some(vec!["txt".to_string()]));

//         // actual
//         let actual = loader.load().await.unwrap();

//         let mut names: Vec<String> = actual.iter().map(|n|
// n.path.file_name().unwrap().to_string_lossy().to_string()).collect();
//         names.sort();

//         let expected = vec!["a.txt".to_string(), "c.txt".to_string()];

//         assert_eq!(names, expected);

//         // also verify original_size matches content length
//         for node in actual {
//             assert_eq!(node.original_size, node.content.len());
//         }
//     }
// }
