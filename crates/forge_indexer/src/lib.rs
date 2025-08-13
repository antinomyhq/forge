/// Core traits for the indexing pipeline
pub mod traits;

/// Domain types for documents, chunks, and embeddings
pub mod domain;

/// Configuration types
pub mod config;

/// Core pipeline implementation
pub mod pipeline;

pub use config::*;
pub use domain::*;
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
}
