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
        use fake::{Fake, Faker};

        // Create a fixture with vectors of different lengths
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

        let fixture = vec![vec![a, b], vec![c], vec![d, e, f]];

        // Execute the function to get the actual result
        let actual = Reasoning::from_parts(fixture);

        // Define the expected result
        let acd: ReasoningFull = Faker.fake();
        let acd = acd.text("a-textc-textd-text").signature("a-sigc-sigd-sig");

        let be: ReasoningFull = Faker.fake();
        let be = be.text("b-texte-text").signature("b-sige-sig");

        let f_full: ReasoningFull = Faker.fake();
        let f_full = f_full.text("f-text").signature("f-sig");

        let expected = vec![acd, be, f_full];

        // Assert that the actual result matches the expected result
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_reasoning_detail_from_parts_with_none_values() {
        use fake::{Fake, Faker};

        // Create a fixture with some None values
        let a: ReasoningPart = Faker.fake();
        let mut a = a.text("a-text");
        a.signature = None;

        let b: ReasoningPart = Faker.fake();
        let mut b = b.signature("b-sig");
        b.text = None;

        let c: ReasoningPart = Faker.fake();
        let mut c = c.text("b-test");
        c.signature = None;

        let fixture = vec![vec![a], vec![b], vec![c]];

        // Execute the function to get the actual result
        let actual = Reasoning::from_parts(fixture);

        // Define the expected result
        let expected_full: ReasoningFull = Faker.fake();
        let expected_full = expected_full.text("a-textb-test").signature("b-sig");
        let expected = vec![expected_full];

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
        use fake::{Fake, Faker};

        let text_only: ReasoningPart = Faker.fake();
        let mut text_only = text_only.text("text-only");
        text_only.signature = None;

        let complete1: ReasoningPart = Faker.fake();
        let complete1 = complete1.text("complete-text").signature("complete-sig");

        let mut empty: ReasoningPart = Faker.fake();
        empty.text = None;
        empty.signature = None;

        let more_text_only: ReasoningPart = Faker.fake();
        let mut more_text_only = more_text_only.text("more-text");
        more_text_only.signature = None;

        let complete2: ReasoningPart = Faker.fake();
        let complete2 = complete2.text("more-text2").signature("more-sig");

        let mut empty2: ReasoningPart = Faker.fake();
        empty2.text = None;
        empty2.signature = None;

        let fixture = vec![
            vec![text_only, complete1, empty],
            vec![more_text_only, complete2, empty2],
        ];

        let actual = Reasoning::from_parts(fixture);

        let expected_full: ReasoningFull = Faker.fake();
        let expected_full = expected_full
            .text("complete-textmore-text2")
            .signature("complete-sigmore-sig");
        let expected = vec![expected_full];

        assert_eq!(actual, expected);
    }
}
