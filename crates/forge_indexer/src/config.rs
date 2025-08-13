/// Configuration for the indexing pipeline
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub chunk_batch_size: usize,
    pub embed_batch_size: usize,
    pub storage_batch_size: usize,
    pub max_concurrent_chunks: usize,
    pub max_concurrent_embeds: usize,
    pub max_concurrent_storage: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            chunk_batch_size: 50,
            embed_batch_size: 100,
            storage_batch_size: 100,
            max_concurrent_chunks: 10,
            max_concurrent_embeds: 5,
            max_concurrent_storage: 5,
        }
    }
}

impl PipelineConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn test() -> Self {
        Self {
            chunk_batch_size: 10,
            embed_batch_size: 20,
            storage_batch_size: 20,
            max_concurrent_chunks: 2,
            max_concurrent_embeds: 2,
            max_concurrent_storage: 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn config_has_sensible_defaults() {
        let fixture = PipelineConfig::default();

        let actual = fixture.embed_batch_size;
        let expected = 100;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_config_has_smaller_values() {
        let fixture = PipelineConfig::test();

        let actual = fixture.max_concurrent_embeds;
        let expected = 2;
        assert_eq!(actual, expected);
    }
}
