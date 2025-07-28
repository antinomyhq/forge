use std::fmt::Display;

use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct ToolName {
    #[serde(flatten)]
    pub name: String,
    #[serde(skip)]
    pub sanitized_name: String,
    #[serde(skip)]
    pub original: String,
}

impl ToolName {
    pub fn new(value: impl ToString) -> Self {
        let name = value.to_string();
        let sanitized_name = Self::sanitize(value.to_string());
        ToolName { name: value.to_string(), sanitized_name, original: name }
    }

    /// Transforms the tool_name to remove whitespaces and converts to
    /// lower_snake_case
    pub fn sanitize(value: impl ToString) -> String {
        let input = value.to_string();

        // Convert to lowercase
        let lower = input.to_lowercase();

        // Replace all non-alphanumeric characters (excluding underscore) with
        // underscores
        let re_special = Regex::new(r"[^a-z0-9_]+").unwrap();
        let cleaned = re_special.replace_all(&lower, "_");

        // Remove leading/trailing underscores and collapse consecutive underscores
        let re_trimmed = Regex::new(r"_+").unwrap();

        re_trimmed
            .replace_all(&cleaned, "_")
            .trim_matches('_')
            .to_string()
    }
}

impl ToolName {
    pub fn into_string(self) -> String {
        self.name
    }

    pub fn as_str(&self) -> &str {
        &self.name
    }

    pub fn sanitized_name(&self) -> &str {
        &self.sanitized_name
    }

    pub fn sanitized_into_string(self) -> String {
        self.sanitized_name
    }
}

impl From<String> for ToolName {
    fn from(value: String) -> Self {
        ToolName::new(value)
    }
}

impl From<&str> for ToolName {
    fn from(value: &str) -> Self {
        ToolName::new(value)
    }
}

pub trait NamedTool {
    fn tool_name() -> ToolName;
}

impl Display for ToolName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_sanitize_camel_case() {
        let fixture = "camelCase";
        let actual = ToolName::sanitize(fixture);
        let expected = "camelcase";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sanitize_pascal_case() {
        let fixture = "PascalCase";
        let actual = ToolName::sanitize(fixture);
        let expected = "pascalcase";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sanitize_mixed_case_with_numbers() {
        let fixture = "myTool2Name";
        let actual = ToolName::sanitize(fixture);
        let expected = "mytool2name";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sanitize_special_characters() {
        let fixture = "tool-name@with#special$chars";
        let actual = ToolName::sanitize(fixture);
        let expected = "tool_name_with_special_chars";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sanitize_whitespace() {
        let fixture = "tool name with spaces";
        let actual = ToolName::sanitize(fixture);
        let expected = "tool_name_with_spaces";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sanitize_consecutive_special_chars() {
        let fixture = "tool---name___with@@@special";
        let actual = ToolName::sanitize(fixture);
        let expected = "tool_name_with_special";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sanitize_leading_trailing_special_chars() {
        let fixture = "___tool_name___";
        let actual = ToolName::sanitize(fixture);
        let expected = "tool_name";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sanitize_already_snake_case() {
        let fixture = "already_snake_case";
        let actual = ToolName::sanitize(fixture);
        let expected = "already_snake_case";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sanitize_uppercase_letters() {
        let fixture = "UPPERCASE_TOOL_NAME";
        let actual = ToolName::sanitize(fixture);
        let expected = "uppercase_tool_name";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sanitize_numbers_only() {
        let fixture = "123456";
        let actual = ToolName::sanitize(fixture);
        let expected = "123456";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sanitize_mixed_numbers_and_letters() {
        let fixture = "tool1Name2Test3";
        let actual = ToolName::sanitize(fixture);
        let expected = "tool1name2test3";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sanitize_empty_string() {
        let fixture = "";
        let actual = ToolName::sanitize(fixture);
        let expected = "";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sanitize_only_special_chars() {
        let fixture = "@#$%^&*()";
        let actual = ToolName::sanitize(fixture);
        let expected = "";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sanitize_complex_mixed_case() {
        let fixture = "XMLHttpRequest2Handler";
        let actual = ToolName::sanitize(fixture);
        let expected = "xmlhttprequest2handler";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sanitize_dots_and_slashes() {
        let fixture = "tool.name/with.dots";
        let actual = ToolName::sanitize(fixture);
        let expected = "tool_name_with_dots";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_new_uses_sanitize() {
        let fixture = "CamelCaseToolName";
        let actual = ToolName::new(fixture);
        let expected = ToolName {
            name: "CamelCaseToolName".to_string(),
            original: "CamelCaseToolName".to_string(),
            sanitized_name: "camelcasetoolname".to_string(),
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sanitize_single_underscore_preserved() {
        let fixture = "tool_name";
        let actual = ToolName::sanitize(fixture);
        let expected = "tool_name";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sanitize_camel_case_with_underscore() {
        let fixture = "camelCase_withUnderscore";
        let actual = ToolName::sanitize(fixture);
        let expected = "camelcase_withunderscore";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sanitize_numbers_between_letters() {
        let fixture = "tool1tool2tool3";
        let actual = ToolName::sanitize(fixture);
        let expected = "tool1tool2tool3";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sanitize_mixed_case_preserves_numbers() {
        let fixture = "Test123Case";
        let actual = ToolName::sanitize(fixture);
        let expected = "test123case";
        assert_eq!(actual, expected);
    }
}
