use derive_setters::Setters;
use serde::{Deserialize, Serialize};

/// Represents a reasoning detail that may be included in the response
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Default, Setters)]
#[setters(into)]
pub struct ReasoningDetail {
    pub text: Option<String>,
    pub signature: Option<String>,
    pub data: Option<String>,
    pub id: Option<String>,
    pub format: Option<String>,
    pub index: Option<i32>,
    pub type_of: Option<String>,
}

/// Type alias for partial reasoning (used in streaming)
pub type ReasoningPart = ReasoningDetail;

/// Type alias for complete reasoning
pub type ReasoningFull = ReasoningDetail;

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
        let mut result: Vec<ReasoningFull> = Vec::new();
        let mut current_text_parts: Vec<ReasoningPart> = Vec::new();

        for part_vec in parts {
            for part in part_vec {
                // According to OpenRouter SDK:
                // 1. Only 'reasoning.text' blocks are merged when consecutive.
                // 2. IMPORTANT: Blocks with signatures or IDs must NEVER be merged,
                //    as this invalidates the cryptographic signature (e.g., for Gemini).
                let is_text = part.type_of.as_deref() == Some("reasoning.text")
                    || (part.type_of.is_none() && part.text.is_some());

                let is_mergeable = is_text && part.signature.is_none() && part.id.is_none();

                if is_mergeable {
                    current_text_parts.push(part);
                } else {
                    // Non-mergeable type or has signature/id. Flush any pending text parts first.
                    if !current_text_parts.is_empty() {
                        if let Some(merged) = Self::merge_parts(
                            Some("reasoning.text".to_string()),
                            &current_text_parts,
                        ) {
                            result.push(merged);
                        }
                        current_text_parts.clear();
                    }

                    // Add this part as a separate block to preserve signature integrity
                    result.push(ReasoningFull {
                        text: part.text,
                        signature: part.signature,
                        data: part.data,
                        id: part.id,
                        format: part.format,
                        index: part.index,
                        type_of: part.type_of,
                    });
                }
            }
        }

        // Flush any remaining text parts
        if !current_text_parts.is_empty()
            && let Some(merged) =
                Self::merge_parts(Some("reasoning.text".to_string()), &current_text_parts)
        {
            result.push(merged);
        }

        result
    }

    fn merge_parts(type_key: Option<String>, parts: &[ReasoningPart]) -> Option<ReasoningFull> {
        // Merge text from all parts
        let text = parts
            .iter()
            .filter_map(|p| p.text.as_deref())
            .collect::<String>();

        // Get first non-empty value for each field
        let signature = parts.iter().find_map(|p| {
            p.signature
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(String::from)
        });
        let id = parts
            .iter()
            .find_map(|p| p.id.as_deref().filter(|s| !s.is_empty()).map(String::from));
        let format = parts.iter().find_map(|p| {
            p.format
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(String::from)
        });
        let index = parts.iter().find_map(|p| p.index);

        // Only include if at least one field has data
        if text.is_empty() && signature.is_none() {
            return None;
        }

        Some(ReasoningFull {
            text: (!text.is_empty()).then_some(text),
            signature,
            data: None,
            id,
            format,
            index,
            type_of: type_key,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reasoning_detail_from_parts_merges_consecutive_types() {
        // Create a fixture with parts of different types across streaming deltas
        let fixture = vec![
            // First delta: reasoning.text
            vec![ReasoningPart {
                type_of: Some("reasoning.text".to_string()),
                text: Some("Part 1 ".to_string()),
                ..Default::default()
            }],
            // Second delta: reasoning.text continues
            vec![ReasoningPart {
                type_of: Some("reasoning.text".to_string()),
                text: Some("Part 2".to_string()),
                ..Default::default()
            }],
            // Third delta: reasoning.encrypted appears
            vec![ReasoningPart {
                type_of: Some("reasoning.encrypted".to_string()),
                data: Some("encrypted_data".to_string()),
                id: Some("tool_call_id".to_string()),
                ..Default::default()
            }],
            // Fourth delta: another reasoning.text appears (non-consecutive with first)
            vec![ReasoningPart {
                type_of: Some("reasoning.text".to_string()),
                text: Some("Part 3".to_string()),
                ..Default::default()
            }],
        ];

        // Execute the function to get the actual result
        let actual = Reasoning::from_parts(fixture);

        // Should have 3 entries: merged text 1+2, encrypted, and separate text 3
        assert_eq!(actual.len(), 3);

        // Verify order and content
        assert_eq!(actual[0].type_of, Some("reasoning.text".to_string()));
        assert_eq!(actual[0].text, Some("Part 1 Part 2".to_string()));

        assert_eq!(actual[1].type_of, Some("reasoning.encrypted".to_string()));
        assert_eq!(actual[1].data, Some("encrypted_data".to_string()));

        assert_eq!(actual[2].type_of, Some("reasoning.text".to_string()));
        assert_eq!(actual[2].text, Some("Part 3".to_string()));
    }

    #[test]
    fn test_reasoning_detail_from_parts_with_different_lengths() {
        // Create a fixture with different types of reasoning
        let fixture = vec![
            vec![ReasoningPart {
                type_of: Some("type1".to_string()),
                text: Some("a-text".to_string()),
                signature: Some("a-sig".to_string()),
                ..Default::default()
            }],
            vec![ReasoningPart {
                type_of: Some("type2".to_string()),
                text: Some("b-text".to_string()),
                signature: Some("b-sig".to_string()),
                ..Default::default()
            }],
            vec![ReasoningPart {
                type_of: Some("type2".to_string()),
                text: Some("c-text".to_string()),
                signature: Some("c-sig".to_string()),
                ..Default::default()
            }],
        ];

        // Execute the function to get the actual result
        let actual = Reasoning::from_parts(fixture);

        // Non-text types are NEVER merged, even if consecutive
        let expected = vec![
            ReasoningFull {
                type_of: Some("type1".to_string()),
                text: Some("a-text".to_string()),
                signature: Some("a-sig".to_string()),
                ..Default::default()
            },
            ReasoningFull {
                type_of: Some("type2".to_string()),
                text: Some("b-text".to_string()),
                signature: Some("b-sig".to_string()),
                ..Default::default()
            },
            ReasoningFull {
                type_of: Some("type2".to_string()),
                text: Some("c-text".to_string()),
                signature: Some("c-sig".to_string()),
                ..Default::default()
            },
        ];

        // Assert that the actual result matches the expected result
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_reasoning_detail_from_parts_with_none_values() {
        // Create a fixture with some None values
        let fixture = vec![
            vec![ReasoningPart {
                text: Some("a-text".to_string()),
                signature: None,
                ..Default::default()
            }],
            vec![ReasoningPart {
                text: None,
                signature: Some("b-sig".to_string()),
                ..Default::default()
            }],
            vec![ReasoningPart {
                text: Some("b-test".to_string()),
                signature: None,
                ..Default::default()
            }],
        ];

        // Execute the function to get the actual result
        let actual = Reasoning::from_parts(fixture);

        // parts with text but no type_of are treated as reasoning.text and merged if
        // consecutive. The middle part has NO text and NO type_of, so it breaks
        // the consecutive text merge.
        let expected = vec![
            ReasoningFull {
                text: Some("a-text".to_string()),
                signature: None,
                type_of: Some("reasoning.text".to_string()),
                ..Default::default()
            },
            ReasoningFull {
                text: None,
                signature: Some("b-sig".to_string()),
                type_of: None,
                ..Default::default()
            },
            ReasoningFull {
                text: Some("b-test".to_string()),
                signature: None,
                type_of: Some("reasoning.text".to_string()),
                ..Default::default()
            },
        ];

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
    fn test_reasoning_detail_from_parts_keeps_partial_reasoning() {
        let fixture = vec![
            vec![
                ReasoningPart {
                    type_of: Some("reasoning.text".to_string()),
                    text: Some("text-only".to_string()),
                    signature: None,
                    ..Default::default()
                },
                ReasoningPart {
                    type_of: Some("reasoning.encrypted".to_string()),
                    data: Some("complete-data".to_string()),
                    signature: Some("complete-sig".to_string()),
                    ..Default::default()
                },
            ],
            vec![
                ReasoningPart {
                    type_of: Some("reasoning.text".to_string()),
                    text: Some("more-text".to_string()),
                    signature: None,
                    ..Default::default()
                },
                ReasoningPart {
                    type_of: Some("reasoning.encrypted".to_string()),
                    data: Some("more-data2".to_string()),
                    signature: Some("more-sig".to_string()),
                    ..Default::default()
                },
            ],
        ];

        let actual = Reasoning::from_parts(fixture);

        // Encrypted reasoning blocks are NEVER merged.
        // Non-consecutive text blocks are NOT merged.
        // In this case: [text1, encrypted1, text2, encrypted2]
        assert_eq!(actual.len(), 4);

        assert_eq!(actual[0].text, Some("text-only".to_string()));
        assert_eq!(actual[1].data, Some("complete-data".to_string()));
        assert_eq!(actual[2].text, Some("more-text".to_string()));
        assert_eq!(actual[3].data, Some("more-data2".to_string()));
    }

    #[test]
    fn test_reasoning_detail_does_not_merge_parts_with_signatures() {
        let fixture = vec![vec![
            ReasoningPart {
                type_of: Some("reasoning.text".to_string()),
                text: Some("Part 1".to_string()),
                signature: Some("sig1".to_string()),
                ..Default::default()
            },
            ReasoningPart {
                type_of: Some("reasoning.text".to_string()),
                text: Some("Part 2".to_string()),
                signature: Some("sig2".to_string()),
                ..Default::default()
            },
        ]];

        let actual = Reasoning::from_parts(fixture);

        // Blocks with signatures should NOT be merged
        assert_eq!(actual.len(), 2);
        assert_eq!(actual[0].text, Some("Part 1".to_string()));
        assert_eq!(actual[0].signature, Some("sig1".to_string()));
        assert_eq!(actual[1].text, Some("Part 2".to_string()));
        assert_eq!(actual[1].signature, Some("sig2".to_string()));
    }
}
