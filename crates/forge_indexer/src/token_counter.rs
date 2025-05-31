use tiktoken_rs::CoreBPE;

/// Token counter for counting tokens in a chunk
pub struct TokenCounter(CoreBPE);

impl TryFrom<&str> for TokenCounter {
    type Error = anyhow::Error;
    fn try_from(model: &str) -> Result<Self, Self::Error> {
        Ok(Self(tiktoken_rs::get_bpe_from_model(model)?))
    }
}

impl TokenCounter {
    /// Return the tokens present in the text
    pub fn count_tokens(&self, text: &str) -> usize {
        self.0.encode_ordinary(text).len()
    }
}
