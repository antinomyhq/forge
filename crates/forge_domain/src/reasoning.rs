use serde::{Deserialize, Serialize};

/// Represents a reasoning detail that may be included in the response
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct ReasoningPart {
    pub text: Option<String>,
    pub signature: Option<String>,
}

/// Represents a reasoning detail that may be included in the response
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct ReasoningFull {
    pub text: Option<String>,
    pub signature: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Reasoning {
    Part(Vec<ReasoningPart>),
    Full(Vec<ReasoningFull>),
}

impl Reasoning {
    pub fn as_partial(&self) -> Option<&Vec<ReasoningPart>> {
        match self {
            Reasoning::Part(parts) => Some(parts),
            Reasoning::Full(_) => None,
        }
    }

    pub fn as_full(&self) -> Option<&Vec<ReasoningFull>> {
        match self {
            Reasoning::Part(_) => None,
            Reasoning::Full(full) => Some(full),
        }
    }

    pub fn from_parts(parts: Vec<Vec<ReasoningPart>>) -> Vec<ReasoningFull> {
        // We merge based on index.
        // eg. [ [a,b,c], [d,e,f], [g,h,i] ] -> [a,d,g], [b,e,h], [c,f,i]
        let max_length = parts.iter().map(Vec::len).max().unwrap_or(0);
        (0..max_length)
            .map(|index| {
                let text = parts
                    .iter()
                    .filter_map(|part_vec| part_vec.get(index)?.text.as_deref())
                    .collect::<String>();

                let signature = parts
                    .iter()
                    .filter_map(|part_vec| part_vec.get(index)?.signature.as_deref())
                    .collect::<String>();

                ReasoningFull {
                    text: (!text.is_empty()).then_some(text),
                    signature: (!signature.is_empty()).then_some(signature),
                }
            })
            .filter(|reasoning| reasoning.text.is_some() && reasoning.signature.is_some())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reasoning_detail_from_parts() {
        // Create a fixture with three vectors of ReasoningDetailPart
        let fixture = vec![
            // First vector [a, b, c]
            vec![
                ReasoningPart {
                    text: Some("a-text".to_string()),
                    signature: Some("a-sig".to_string()),
                },
                ReasoningPart {
                    text: Some("b-text".to_string()),
                    signature: Some("b-sig".to_string()),
                },
                ReasoningPart {
                    text: Some("c-text".to_string()),
                    signature: Some("c-sig".to_string()),
                },
            ],
            // Second vector [d, e, f]
            vec![
                ReasoningPart {
                    text: Some("d-text".to_string()),
                    signature: Some("d-sig".to_string()),
                },
                ReasoningPart {
                    text: Some("e-text".to_string()),
                    signature: Some("e-sig".to_string()),
                },
                ReasoningPart {
                    text: Some("f-text".to_string()),
                    signature: Some("f-sig".to_string()),
                },
            ],
            // Third vector [g, h, i]
            vec![
                ReasoningPart {
                    text: Some("g-text".to_string()),
                    signature: Some("g-sig".to_string()),
                },
                ReasoningPart {
                    text: Some("h-text".to_string()),
                    signature: Some("h-sig".to_string()),
                },
                ReasoningPart {
                    text: Some("i-text".to_string()),
                    signature: Some("i-sig".to_string()),
                },
            ],
        ];

        // Execute the function to get the actual result
        let actual = Reasoning::from_parts(fixture);

        // Define the expected result
        let expected = vec![
            // First merged vector [a, d, g]
            ReasoningFull {
                text: Some("a-textd-textg-text".to_string()),
                signature: Some("a-sigd-sigg-sig".to_string()),
            },
            // Second merged vector [b, e, h]
            ReasoningFull {
                text: Some("b-texte-texth-text".to_string()),
                signature: Some("b-sige-sigh-sig".to_string()),
            },
            // Third merged vector [c, f, i]
            ReasoningFull {
                text: Some("c-textf-texti-text".to_string()),
                signature: Some("c-sigf-sigi-sig".to_string()),
            },
        ];

        // Assert that the actual result matches the expected result
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_reasoning_detail_from_parts_with_different_lengths() {
        // Create a fixture with vectors of different lengths
        let fixture = vec![
            vec![
                ReasoningPart {
                    text: Some("a-text".to_string()),
                    signature: Some("a-sig".to_string()),
                },
                ReasoningPart {
                    text: Some("b-text".to_string()),
                    signature: Some("b-sig".to_string()),
                },
            ],
            vec![ReasoningPart {
                text: Some("c-text".to_string()),
                signature: Some("c-sig".to_string()),
            }],
            vec![
                ReasoningPart {
                    text: Some("d-text".to_string()),
                    signature: Some("d-sig".to_string()),
                },
                ReasoningPart {
                    text: Some("e-text".to_string()),
                    signature: Some("e-sig".to_string()),
                },
                ReasoningPart {
                    text: Some("f-text".to_string()),
                    signature: Some("f-sig".to_string()),
                },
            ],
        ];

        // Execute the function to get the actual result
        let actual = Reasoning::from_parts(fixture);

        // Define the expected result
        let expected = vec![
            // First merged vector [a, c, d]
            ReasoningFull {
                text: Some("a-textc-textd-text".to_string()),
                signature: Some("a-sigc-sigd-sig".to_string()),
            },
            // Second merged vector [b, e]
            ReasoningFull {
                text: Some("b-texte-text".to_string()),
                signature: Some("b-sige-sig".to_string()),
            },
            // Third merged vector [f]
            ReasoningFull {
                text: Some("f-text".to_string()),
                signature: Some("f-sig".to_string()),
            },
        ];

        // Assert that the actual result matches the expected result
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_reasoning_detail_from_parts_with_none_values() {
        // Create a fixture with some None values
        let fixture = vec![
            vec![ReasoningPart { text: Some("a-text".to_string()), signature: None }],
            vec![ReasoningPart { text: None, signature: Some("b-sig".to_string()) }],
            vec![ReasoningPart { text: Some("b-test".to_string()), signature: None }],
        ];

        // Execute the function to get the actual result
        let actual = Reasoning::from_parts(fixture);

        // Define the expected result
        let expected = vec![ReasoningFull {
            text: Some("a-textb-test".to_string()),
            signature: Some("b-sig".to_string()),
        }];

        // Assert that the actual result matches the expected result
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_reasoning_detail_from_empty_parts() {
        // Empty fixture
        let fixture: Vec<Vec<ReasoningPart>> = vec![];

        // Execute the function to get the actual result
        let actual = Reasoning::from_parts(fixture);

        // Define the expected result - should be an empty vector
        let expected: Vec<ReasoningFull> = vec![];

        // Assert that the actual result matches the expected result
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_reasoning_detail_from_parts_filters_incomplete_reasoning() {
        let fixture = vec![
            vec![
                ReasoningPart { text: Some("text-only".to_string()), signature: None },
                ReasoningPart {
                    text: Some("complete-text".to_string()),
                    signature: Some("complete-sig".to_string()),
                },
                ReasoningPart { text: None, signature: None },
            ],
            vec![
                ReasoningPart { text: Some("more-text".to_string()), signature: None },
                ReasoningPart {
                    text: Some("more-text2".to_string()),
                    signature: Some("more-sig".to_string()),
                },
                ReasoningPart { text: None, signature: None },
            ],
        ];

        let actual = Reasoning::from_parts(fixture);

        let expected = vec![ReasoningFull {
            text: Some("complete-textmore-text2".to_string()),
            signature: Some("complete-sigmore-sig".to_string()),
        }];
        assert_eq!(actual, expected);
    }
}
