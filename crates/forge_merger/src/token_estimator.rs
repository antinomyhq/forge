//! Token estimation strategies for sharded processing.

/// Token estimation strategies for sharded processing.
#[derive(Clone, Debug)]
pub enum TokenEstimator {
    /// Estimates tokens based on word count multiplied by a factor (default:
    /// 0.75).
    WordCountBased(f32),
    /// Estimates tokens based on character count divided by a factor (default:
    /// 4.0).
    CharacterBased(f32),
    /// Uses a custom function to estimate tokens.
    Custom(fn(&str) -> usize),
}

impl Default for TokenEstimator {
    fn default() -> Self {
        TokenEstimator::WordCountBased(0.75)
    }
}

impl TokenEstimator {
    /// Estimate the number of tokens in a string.
    pub fn estimate(&self, content: &str) -> usize {
        match self {
            Self::WordCountBased(multiplier) => {
                ((content.split_whitespace().count() as f32) * multiplier).round() as usize
            }
            Self::CharacterBased(chars_per_token) => {
                ((content.chars().count() as f32) / chars_per_token).round() as usize
            }
            Self::Custom(estimator) => estimator(content),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_estimation() {
        let samples = [
            ("Hello world", 2), // 2 words * 0.75 ≈ 2
            (
                "fn main() {\n    println!(\"Hello, world!\");\n}",
                5,
            ), // 6 words * 0.75 rounded ≈ 5
            (
                "This is a longer text with multiple sentences. It should have a higher token count.",
                11,
            ), // 15 words * 0.75 rounded ≈ 11
        ];

        for (sample, expected) in samples {
            let estimator = TokenEstimator::WordCountBased(0.75);
            let estimated = estimator.estimate(sample);
            assert_eq!(estimated, expected, "Failed on sample: {}", sample);
        }
    }

    #[test]
    fn test_character_based_token_estimation() {
        let samples = [
            ("Hello world", 3), // 11 chars / 4.0 ≈ 3
            ("fn main() {\n    println!(\"Hello, world!\");\n}", 11), /* 42 chars / 4.0 ≈ 10.5,
                                 * rounded to 11 */
        ];

        for (sample, expected) in samples {
            let estimator = TokenEstimator::CharacterBased(4.0);
            let estimated = estimator.estimate(sample);
            assert_eq!(estimated, expected, "Failed on sample: {}", sample);
        }
    }

    #[test]
    fn test_custom_token_estimation() {
        // A simple custom estimator that just returns the length of the string
        fn custom_estimator(content: &str) -> usize {
            content.len()
        }

        let samples = [("Hello world", 11), ("A", 1)];

        for (sample, expected) in samples {
            let estimator = TokenEstimator::Custom(custom_estimator);
            let estimated = estimator.estimate(sample);
            assert_eq!(estimated, expected, "Failed on sample: {}", sample);
        }
    }
}
