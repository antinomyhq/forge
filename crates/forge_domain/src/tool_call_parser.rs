use std::collections::HashMap;

use nom::bytes::complete::{tag, take_until, take_while1};
use nom::character::complete::multispace0;
use nom::multi::many0;
use nom::{IResult, Parser};
use serde_json::Value;

use super::ToolCallFull;
use crate::ToolName;

#[derive(Debug, PartialEq)]
pub struct ToolCallParsed {
    pub name: String,
    pub args: HashMap<String, String>,
}

// Allow alphanumeric and underscore characters
fn is_identifier_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn parse_identifier(input: &str) -> IResult<&str, &str> {
    take_while1(is_identifier_char).parse(input)
}

fn parse_arg(input: &str) -> IResult<&str, (&str, &str)> {
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("<").parse(input)?;
    let (input, key) = parse_identifier(input)?;
    let (input, _) = tag(">").parse(input)?;
    let (input, _) = multispace0(input)?; // Handle whitespace after opening tag

    let (input, value) = take_until("</").parse(input)?;
    let (input, _) = tag("</").parse(input)?;
    let (input, _) = tag(key).parse(input)?;
    let (input, _) = tag(">").parse(input)?;
    let (input, _) = multispace0(input)?;

    Ok((input, (key, value)))
}

fn parse_args(input: &str) -> IResult<&str, HashMap<String, String>> {
    let mut arg_parser = many0(parse_arg);
    let (input, args) = arg_parser.parse(input)?;
    let mut map = HashMap::new();
    for (key, value) in args {
        // Clean up any extraneous whitespace in values, including indentation and
        // newlines
        map.insert(
            key.to_string(),
            value.split_whitespace().collect::<Vec<_>>().join(" "),
        );
    }
    Ok((input, map))
}

fn parse_tool_call(input: &str) -> IResult<&str, ToolCallParsed> {
    let (input, _) = multispace0(input)?; // Handle leading whitespace and newlines
    let (input, _) = tag("<tool_call>").parse(input)?;
    let (input, _) = multispace0(input)?; // Handle whitespace after <tool_call>

    // Match the tool name tags: <tool_name>
    let (input, _) = tag("<").parse(input)?;
    let (input, name) = parse_identifier(input)?;
    let (input, _) = tag(">").parse(input)?;
    let (input, _) = multispace0(input)?;

    // Match all the arguments with whitespace
    let (input, args) = parse_args(input)?;

    // Match closing tags: </tool_name>
    let (input, _) = multispace0(input)?; // Handle whitespace before closing tag
    let (input, _) = tag("</").parse(input)?;
    let (input, _) = tag(name).parse(input)?;
    let (input, _) = tag(">").parse(input)?;
    let (input, _) = multispace0(input)?; // Handle whitespace after closing tag

    // Match </tool_call> and any whitespace
    let (input, _) = tag("</tool_call>").parse(input)?;
    let (input, _) = multispace0(input)?; // Handle trailing whitespace and newlines

    Ok((
        input,
        ToolCallParsed {
            name: name.to_string(),
            args: args
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.trim().to_string()))
                .collect(),
        },
    ))
}

fn find_next_tool_call(input: &str) -> IResult<&str, &str> {
    // Find the next occurrence of a tool call opening tag
    let (remaining, _) = take_until("<tool_call>").parse(input)?;
    Ok((remaining, ""))
}

fn convert_string_to_value(value: &str) -> Value {
    // Try to parse as boolean first
    match value.trim().to_lowercase().as_str() {
        "true" => return Value::Bool(true),
        "false" => return Value::Bool(false),
        _ => {}
    }

    // Try to parse as number
    if let Ok(int_val) = value.parse::<i64>() {
        return Value::Number(int_val.into());
    }

    if let Ok(float_val) = value.parse::<f64>() {
        // Create number from float, handling special case where float is actually an
        // integer
        return if float_val.fract() == 0.0 {
            Value::Number(serde_json::Number::from(float_val as i64))
        } else if let Some(num) = serde_json::Number::from_f64(float_val) {
            Value::Number(num)
        } else {
            Value::String(value.to_string())
        };
    }

    // Default to string if no other type matches
    Value::String(value.to_string())
}

fn tool_call_to_struct(parsed: ToolCallParsed) -> ToolCallFull {
    ToolCallFull {
        name: ToolName::new(parsed.name),
        call_id: None,
        arguments: Value::Object(parsed.args.into_iter().fold(
            serde_json::Map::new(),
            |mut map, (key, value)| {
                map.insert(key, convert_string_to_value(&value));
                map
            },
        )),
    }
}

pub fn parse(input: &str) -> Result<Vec<ToolCallFull>, String> {
    let mut tool_calls = Vec::new();
    let mut current_input = input;

    while !current_input.is_empty() {
        // Try to find the next tool call
        match find_next_tool_call(current_input) {
            Ok((remaining, _)) => {
                // Try to parse a tool call at the current position
                match parse_tool_call(remaining) {
                    Ok((new_remaining, parsed)) => {
                        tool_calls.push(tool_call_to_struct(parsed));
                        current_input = new_remaining;
                    }
                    Err(e) => {
                        if tool_calls.is_empty() {
                            return Err(format!("Failed to parse tool call: {}", e));
                        }
                        // If we've already found some tool calls, we can stop here
                        break;
                    }
                }
            }
            Err(_) => break, // No more tool calls found
        }
    }

    if tool_calls.is_empty() {
        Ok(Vec::new())
    } else {
        Ok(tool_calls)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::ToolName;

    // Test helpers
    struct ToolCallBuilder {
        name: String,
        args: HashMap<String, String>,
    }

    impl ToolCallBuilder {
        fn new(name: &str) -> Self {
            Self { name: name.to_string(), args: HashMap::new() }
        }

        fn arg(mut self, key: &str, value: &str) -> Self {
            self.args.insert(key.to_string(), value.to_string());
            self
        }

        fn build_xml(&self) -> String {
            let mut xml = String::from("<tool_call>");
            xml.push_str(&format!("<{}>", self.name));
            let args: Vec<_> = self.args.iter().collect();
            for (idx, (key, value)) in args.iter().enumerate() {
                xml.push_str(&format!(
                    "<{}>{}</{}>{}",
                    key,
                    value,
                    key,
                    if idx < args.len() - 1 { " " } else { "" }
                ));
            }
            xml.push_str(&format!("</{}></tool_call>", self.name));
            xml
        }

        fn build_expected(&self) -> ToolCallFull {
            let mut args = Value::Object(Default::default());
            for (key, value) in &self.args {
                args.as_object_mut()
                    .unwrap()
                    .insert(key.clone(), convert_string_to_value(value));
            }
            ToolCallFull {
                name: ToolName::new(&self.name),
                call_id: None,
                arguments: args,
            }
        }
    }

    #[test]
    fn test_parse_arg() {
        let action = parse_arg("<key>value</key>").unwrap();
        let expected = ("", ("key", "value"));
        assert_eq!(action, expected);
    }

    #[test]
    fn test_parse_args() {
        let action = parse_args("<key1>value1</key1> <key2>value2</key2>")
            .unwrap()
            .1;
        let expected = {
            let mut map = HashMap::new();
            map.insert("key1".to_string(), "value1".to_string());
            map.insert("key2".to_string(), "value2".to_string());
            map
        };
        assert_eq!(action, expected);
    }

    #[test]
    fn test_actual_llm_respone() {
        // Test with real LLM response including newlines and indentation
        let str = r#"To find the cat hidden in the codebase, I will use the `tool_forge_fs_search` to grep for the string "cat" in all markdown files except those in the `docs` directory.
                <analysis>
                Files Read: */*.md
                Git Status: Not applicable, as we are not dealing with version control changes.
                Compilation Status: Not applicable, as this is a text search.
                Test Status: Not applicable, as this is a text search.
                </analysis>

                <tool_call>
                <tool_forge_fs_search>
                <file_pattern>**/*.md</file_pattern>
                <path>/Users/amit/code-forge</path>
                <regex>cat</regex>
                </tool_forge_fs_search>
                </tool_call>"#;

        let action = parse(str).unwrap();

        let expected = vec![ToolCallFull {
            name: ToolName::new("tool_forge_fs_search"),
            call_id: None,
            arguments: serde_json::from_str(
                r#"{"file_pattern":"**/*.md","path":"/Users/amit/code-forge","regex":"cat"}"#,
            )
            .unwrap(),
        }];
        assert_eq!(action, expected);
    }

    #[test]
    fn test_parse_tool_call() {
        let tool = ToolCallBuilder::new("tool_name")
            .arg("arg1", "value1")
            .arg("arg2", "value2");

        let action = parse_tool_call(&tool.build_xml()).unwrap().1;
        let expected = ToolCallParsed { name: "tool_name".to_string(), args: tool.args };
        assert_eq!(action, expected);
    }

    #[test]
    fn test_parse() {
        let tool = ToolCallBuilder::new("tool_name")
            .arg("arg1", "value1")
            .arg("arg2", "value2");

        let action = parse(&tool.build_xml()).unwrap();
        let expected = vec![tool.build_expected()];
        assert_eq!(action, expected);
    }

    #[test]
    fn test_parse_with_surrounding_text() {
        let tool = ToolCallBuilder::new("tool_name").arg("arg1", "value1");
        let input = format!("Some text {} more text", tool.build_xml());

        let action = parse(&input).unwrap();
        let expected = vec![tool.build_expected()];
        assert_eq!(action, expected);
    }

    #[test]
    fn test_parse_multiple_tool_calls() {
        let tool1 = ToolCallBuilder::new("tool1").arg("arg1", "value1");
        let tool2 = ToolCallBuilder::new("tool2").arg("arg2", "value2");
        let input = format!("{} Some text {}", tool1.build_xml(), tool2.build_xml());

        let action = parse(&input).unwrap();
        let expected = vec![tool1.build_expected(), tool2.build_expected()];
        assert_eq!(action, expected);
    }

    #[test]
    fn test_parse_with_numeric_values() {
        let tool = ToolCallBuilder::new("tool_name")
            .arg("int_value", "42")
            .arg("float_value", "3.14")
            .arg("large_int", "9223372036854775807")
            .arg("zero", "0")
            .arg("negative", "-123");

        let action = parse(&tool.build_xml()).unwrap();
        let expected = vec![tool.build_expected()];
        assert_eq!(action, expected);

        if let Value::Object(map) = &action[0].arguments {
            assert!(matches!(map["int_value"], Value::Number(_)));
            assert!(matches!(map["float_value"], Value::Number(_)));
            assert!(matches!(map["large_int"], Value::Number(_)));
            assert!(matches!(map["zero"], Value::Number(_)));
            assert!(matches!(map["negative"], Value::Number(_)));
        }
    }

    #[test]
    fn test_parse_with_boolean_values() {
        let tool = ToolCallBuilder::new("tool_name")
            .arg("bool1", "true")
            .arg("bool2", "false")
            .arg("bool3", "True")
            .arg("bool4", "FALSE");

        let action = parse(&tool.build_xml()).unwrap();
        let expected = vec![tool.build_expected()];
        assert_eq!(action, expected);

        if let Value::Object(map) = &action[0].arguments {
            assert_eq!(map["bool1"], Value::Bool(true));
            assert_eq!(map["bool2"], Value::Bool(false));
            assert_eq!(map["bool3"], Value::Bool(true));
            assert_eq!(map["bool4"], Value::Bool(false));
        }
    }

    #[test]
    fn test_parse_with_mixed_types() {
        let tool = ToolCallBuilder::new("tool_name")
            .arg("text", "hello")
            .arg("number", "42")
            .arg("float", "3.14")
            .arg("bool", "true")
            .arg("complex", "not_a_number");

        let action = parse(&tool.build_xml()).unwrap();
        let expected = vec![tool.build_expected()];
        assert_eq!(action, expected);

        if let Value::Object(map) = &action[0].arguments {
            assert!(matches!(map["text"], Value::String(_)));
            assert!(matches!(map["number"], Value::Number(_)));
            assert!(matches!(map["float"], Value::Number(_)));
            assert!(matches!(map["bool"], Value::Bool(_)));
            assert!(matches!(map["complex"], Value::String(_)));
        }
    }

    #[test]
    fn test_parse_empty_args() {
        let tool = ToolCallBuilder::new("tool_name");

        let action = parse(&tool.build_xml()).unwrap();
        let expected = vec![tool.build_expected()];
        assert_eq!(action, expected);
    }

    #[test]
    fn test_parse_with_special_chars() {
        let tool = ToolCallBuilder::new("tool_name")
            .arg("arg1", "value with spaces")
            .arg("arg2", "value&with#special@chars");

        let action = parse(&tool.build_xml()).unwrap();
        let expected = vec![tool.build_expected()];
        assert_eq!(action, expected);
    }

    #[test]
    fn test_parse_with_large_text_between() {
        let tool1 = ToolCallBuilder::new("tool1").arg("arg1", "value1");
        let tool2 = ToolCallBuilder::new("tool2").arg("arg2", "value2");
        let input = format!(
            "{}\nLots of text here...\nMore text...\nEven more text...\n{}",
            tool1.build_xml(),
            tool2.build_xml()
        );

        let action = parse(&input).unwrap();
        let expected = vec![tool1.build_expected(), tool2.build_expected()];
        assert_eq!(action, expected);
    }

    #[test]
    fn test_parse_new_tool_call_format() {
        let input = r#"<tool_call><tool_forge_fs_search><path>/test/path</path><regex>test</regex></tool_forge_fs_search></tool_call>"#;

        let action = parse(input).unwrap();
        let expected = vec![ToolCallFull {
            name: ToolName::new("tool_forge_fs_search"),
            call_id: None,
            arguments: serde_json::from_str(r#"{"path":"/test/path","regex":"test"}"#).unwrap(),
        }];
        assert_eq!(action, expected);
    }
}
