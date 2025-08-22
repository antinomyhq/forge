use text_splitter::{Characters, ChunkConfig};

use crate::{Chunk, Chunker, Document, Position};

pub struct CodeSplitter(text_splitter::CodeSplitter<Characters>);

impl CodeSplitter {
    pub fn new(max_size: usize) -> Self {
        let splitter = text_splitter::CodeSplitter::new(
            tree_sitter_rust::LANGUAGE,
            ChunkConfig::new(max_size),
        )
        .unwrap();

        Self(splitter)
    }
}

impl Chunker for CodeSplitter {
    type Input = Document;
    type Output = Chunk;
    fn chunk(&self, input: Self::Input) -> Vec<Self::Output> {
        let chunks = self
            .0
            .chunk_char_indices(&input.content)
            .map(|chunk| {
                Chunk::new(
                    input.path.clone(),
                    chunk.chunk,
                    Position::new(chunk.char_offset, chunk.char_offset + chunk.chunk.len()),
                )
            })
            .collect::<Vec<_>>();

        println!(
            "chunked document({}) into {} chunks",
            input.path.display(),
            chunks.len()
        );

        chunks
    }
}
