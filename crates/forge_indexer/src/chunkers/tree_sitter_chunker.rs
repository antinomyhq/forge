use forge_treesitter::Parser;
use tracing::{error, info};

use super::Chunker;
use crate::{FileLoad, TokenCounter};

pub struct TreeSitterChunker {
    tokenizer: TokenCounter,
    max_tokens: usize,
}

impl TreeSitterChunker {
    pub fn try_new(model: &str, max_tokens: usize) -> anyhow::Result<Self> {
        Ok(Self { tokenizer: TokenCounter::try_from(model)?, max_tokens })
    }
}

#[async_trait::async_trait]
impl Chunker for TreeSitterChunker {
    async fn chunk(&self, input: Vec<FileLoad>) -> anyhow::Result<Vec<forge_treesitter::Block>> {
        // TODO: can be done in parallel.
        let blocks = input
            .into_iter()
            .filter_map(|file| {
                file.path
                    .extension()
                    .map(|ext| ext.to_string_lossy().to_string())
                    .and_then(|ext| {
                        Parser::try_from(ext.as_str())
                            .map_err(|e| error!("failed to create parser: {e}"))
                            .ok()
                            .and_then(|mut parser| {
                                parser
                                    .parse(&file.path, &file.content)
                                    .map_err(|e| error!("failed to parse file contents: {e}"))
                                    .ok()
                            })
                    })
            })
            .flatten()
            .collect::<Vec<_>>();

        let total_blocks = blocks.len();
        info!("Total code blocks: {}", total_blocks);

        let filter_blocks = blocks
            .into_iter()
            .filter(|block| self.tokenizer.count_tokens(&block.snippet) <= self.max_tokens)
            .collect::<Vec<_>>();

        let filtered_blocks_count = filter_blocks.len();

        info!(
            "After filtering {} blocks eliminated, total blocks: {} ",
            total_blocks - filtered_blocks_count,
            filtered_blocks_count
        );

        Ok(filter_blocks)
    }
}
