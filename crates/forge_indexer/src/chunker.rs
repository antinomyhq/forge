use text_splitter::ChunkConfig;
use tiktoken_rs::CoreBPE;

use crate::transform::Transform;
use crate::{Chunk, Document, Position};

pub struct CodeSplitter {
    tokenizer: CoreBPE,
    splitter: text_splitter::CodeSplitter<CoreBPE>,
    min_size: usize,
}

impl CodeSplitter {
    pub fn new(max_size: usize, min_size: usize) -> Self {
        use tiktoken_rs::o200k_base;
        let tokenizer = o200k_base().unwrap();
        let splitter = text_splitter::CodeSplitter::new(
            tree_sitter_rust::LANGUAGE,
            ChunkConfig::new(max_size).with_sizer(tokenizer.clone()),
        )
        .unwrap();

        Self { tokenizer, splitter, min_size }
    }
}

impl Transform for CodeSplitter {
    type In = Vec<Document>;
    type Out = Vec<Chunk>;
    async fn transform(self, input: Self::In) -> anyhow::Result<Self::Out> {
        let out = input
            .into_iter()
            .flat_map(|document| {
                self.splitter
                    .chunk_char_indices(&document.content)
                    .filter(|chunk| {
                        // filter out small chunks that don't contribute anything.
                        self.tokenizer.encode_ordinary(chunk.chunk).len() > self.min_size
                    })
                    .map(|chunk| {
                        Chunk::new(
                            document.path.clone(),
                            chunk.chunk,
                            Position::new(chunk.char_offset, chunk.char_offset + chunk.chunk.len()),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        Ok(out)
    }
}
