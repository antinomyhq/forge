use regex::Regex;

pub fn sanitize_for_json(input: &str) -> String {
    let re = Regex::new(r"[\x00-\x1F]").unwrap();
    re.replace_all(input, "").to_string()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_sanitize_for_json_no_control_characters() {
        let fixture = "Hello, World! This is a normal string with symbols: !@#$%^&*()";
        let actual = sanitize_for_json(fixture);
        let expected = "Hello, World! This is a normal string with symbols: !@#$%^&*()";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sanitize_for_json_removes_invisible_null_character() {
        let fixture = "Hello\x00World";
        let actual = sanitize_for_json(fixture);
        let expected = "HelloWorld";
        assert_eq!(actual, expected);

        assert_eq!(fixture.len(), 11);
        assert_eq!(actual.len(), 10);
    }

    #[test]
    fn test_sanitize_for_json_empty_and_edge_cases() {
        // Empty string
        let fixture = "";
        let actual = sanitize_for_json(fixture);
        let expected = "";
        assert_eq!(actual, expected);

        // Only control characters
        let fixture = "\x00\x01\x09\x0A\x1F";
        let actual = sanitize_for_json(fixture);
        let expected = "";
        assert_eq!(actual, expected);

        // Boundary test: \x1F is last control char, \x20 is space (should be preserved)
        let fixture = "Before\x1FAfter\x20Space";
        let actual = sanitize_for_json(fixture);
        let expected = "BeforeAfter Space";
        assert_eq!(actual, expected);
    }
}
