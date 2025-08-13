/// Core traits for the indexing pipeline
pub mod traits;

/// Domain types for documents, chunks, and embeddings
pub mod domain;

/// Configuration types
pub mod config;

/// File loader implementations
pub mod loader;
/// Core pipeline implementation
pub mod pipeline;

pub use config::*;
pub use domain::*;
pub use loader::*;
pub use pipeline::*;
pub use traits::*;

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn can_create_document() {
        let fixture = Document::test();
        let actual = fixture.path;
        let expected = PathBuf::from("test.rs");
        assert_eq!(actual, expected);
    }
    #[tokio::test]
    async fn can_use_file_loader() {
        use futures::StreamExt;

        let fixture = FileLoader::new(
            FileConfig::new("./src")
                .extensions(vec!["rs".to_string()])
                .ignore_patterns(vec!["target".to_string()]),
        );

        let documents: Vec<_> = fixture.load().take(1).collect().await;
        let actual = documents.len();
        let expected = 1;
        assert_eq!(actual, expected);

        // Verify we got a document
        let document = documents.into_iter().next().unwrap().unwrap();
        let actual = document.content.is_empty();
        let expected = false;
        assert_eq!(actual, expected);
    }
}
