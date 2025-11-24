use serde::{Deserialize, Serialize};

/// Represents a reasoning detail that may be included in the response
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, fake::Dummy, derive_setters::Setters)]
#[setters(into, strip_option)]
pub struct ReasoningPart {
    pub text: Option<String>,
    pub signature: Option<String>,
}

/// Represents a reasoning detail that may be included in the response
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, fake::Dummy, derive_setters::Setters)]
#[setters(into, strip_option)]
pub struct ReasoningFull {
    pub text: Option<String>,
    pub signature: Option<String>,
}

#[derive(Clone, Debug, PartialEq, fake::Dummy)]
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
        use fake::{Fake, Faker};

        // Create a fixture with three vectors of ReasoningDetailPart
        let a: ReasoningPart = Faker.fake();
        let a = a.text("a-text").signature("a-sig");

        let b: ReasoningPart = Faker.fake();
        let b = b.text("b-text").signature("b-sig");

        let c: ReasoningPart = Faker.fake();
        let c = c.text("c-text").signature("c-sig");

        let d: ReasoningPart = Faker.fake();
        let d = d.text("d-text").signature("d-sig");

        let e: ReasoningPart = Faker.fake();
        let e = e.text("e-text").signature("e-sig");

        let f: ReasoningPart = Faker.fake();
        let f = f.text("f-text").signature("f-sig");

        let g: ReasoningPart = Faker.fake();
        let g = g.text("g-text").signature("g-sig");

        let h: ReasoningPart = Faker.fake();
        let h = h.text("h-text").signature("h-sig");

        let i: ReasoningPart = Faker.fake();
        let i = i.text("i-text").signature("i-sig");

        let fixture = vec![vec![a, b, c], vec![d, e, f], vec![g, h, i]];

        // Execute the function to get the actual result
        let actual = Reasoning::from_parts(fixture);

        // Define the expected result
        let adg: ReasoningFull = Faker.fake();
        let adg = adg.text("a-textd-textg-text").signature("a-sigd-sigg-sig");

        let beh: ReasoningFull = Faker.fake();
        let beh = beh.text("b-texte-texth-text").signature("b-sige-sigh-sig");

        let cfi: ReasoningFull = Faker.fake();
        let cfi = cfi.text("c-textf-texti-text").signature("c-sigf-sigi-sig");

        let expected = vec![adg, beh, cfi];

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
