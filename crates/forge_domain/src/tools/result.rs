use derive_setters::Setters;
use forge_template::{ElementBuilder, Output};
use serde::{Deserialize, Serialize};

use crate::{ConversationId, Image, ToolCallFull, ToolCallId, ToolName};

const REFLECTION_PROMPT: &str =
    include_str!("../../../../templates/forge-partial-tool-error-reflection.md");

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Setters)]
#[setters(into)]
pub struct ToolResult {
    pub name: ToolName,
    pub call_id: Option<ToolCallId>,
    #[setters(skip)]
    pub output: ToolOutput,
}

impl ToolResult {
    pub fn new(name: impl Into<ToolName>) -> ToolResult {
        Self {
            name: name.into(),
            call_id: Default::default(),
            output: Default::default(),
        }
    }

    pub fn success(mut self, content: impl Into<String>) -> Self {
        self.output = ToolOutput::text(content.into());

        self
    }

    pub fn failure(self, err: anyhow::Error) -> Self {
        self.output(Err(err))
    }

    pub fn is_error(&self) -> bool {
        self.output.is_error
    }

    pub fn output(mut self, result: Result<ToolOutput, anyhow::Error>) -> Self {
        match result {
            Ok(output) => {
                self.output = output;
            }
            Err(err) => {
                let mut message = vec![err.to_string()];
                let mut source = err.source();
                if source.is_some() {
                    message.push("\nCaused by:".to_string());
                }
                let mut i = 0;
                while let Some(err) = source {
                    message.push(format!("    {i}: {err}"));
                    source = err.source();
                    i += 1;
                }

                self.output = ToolOutput::text(
                    Output::new()
                        .element("tool_call_error")
                        .child(
                            ElementBuilder::new("cause")
                                .cdata(message.join("\n"))
                                .build()
                        )
                        .child(
                            ElementBuilder::new("reflection")
                                .text(REFLECTION_PROMPT)
                                .build()
                        )
                        .done()
                        .render_xml(),
                )
                .is_error(true);
            }
        }
        self
    }
}

impl From<ToolCallFull> for ToolResult {
    fn from(value: ToolCallFull) -> Self {
        Self {
            name: value.name,
            call_id: value.call_id,
            output: Default::default(),
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Setters)]
#[setters(into, strip_option)]
pub struct ToolOutput {
    pub is_error: bool,
    pub values: Vec<ToolValue>,
}

impl ToolOutput {
    pub fn text(tool: impl ToString) -> Self {
        ToolOutput {
            is_error: Default::default(),
            values: vec![ToolValue::Text(tool.to_string())],
        }
    }

    pub fn ai(id: ConversationId, output: impl ToString) -> Self {
        ToolOutput {
            is_error: Default::default(),
            values: vec![ToolValue::AI { value: output.to_string(), conversation_id: id }],
        }
    }

    pub fn image(img: Image) -> Self {
        ToolOutput { is_error: false, values: vec![ToolValue::Image(img)] }
    }

    /// Creates a ToolOutput with a Markdown value
    pub fn markdown(md: impl ToString) -> Self {
        ToolOutput { is_error: false, values: vec![ToolValue::Markdown(md.to_string())] }
    }

    /// Creates a paired output with XML for LLM and Markdown for display
    pub fn paired(xml: impl ToString, markdown: impl ToString) -> Self {
        ToolOutput {
            is_error: false,
            values: vec![ToolValue::Pair(
                Box::new(ToolValue::Text(xml.to_string())),
                Box::new(ToolValue::Markdown(markdown.to_string())),
            )],
        }
    }

    /// Creates a paired output from an Output and Markdown string
    pub fn from_output_and_markdown(output: &Output, markdown: impl ToString) -> Self {
        Self::paired(output.render_xml(), markdown.to_string())
    }
    
    /// Creates a paired output from an Output (renders both XML and Markdown)
    pub fn from_output(output: &Output) -> Self {
        Self::paired(output.render_xml(), output.render_markdown())
    }

    pub fn combine_mut(&mut self, value: ToolOutput) {
        self.values.extend(value.values);
    }

    pub fn combine(self, other: ToolOutput) -> Self {
        let mut items = self.values;
        items.extend(other.values);
        ToolOutput { values: items, is_error: self.is_error || other.is_error }
    }

    /// Returns the first item as a string if it exists
    pub fn as_str(&self) -> Option<&str> {
        self.values.iter().find_map(|item| item.as_str())
    }
}

impl<T> From<T> for ToolOutput
where
    T: Iterator<Item = ToolOutput>,
{
    fn from(item: T) -> Self {
        item.fold(ToolOutput::default(), |acc, item| acc.combine(item))
    }
}

/// Represents a file diff with old and new content
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct FileDiff {
    /// Path to the file
    pub path: String,
    /// Old content of the file (None if file is new)
    pub old_text: Option<String>,
    /// New content of the file
    pub new_text: String,
}

/// Like serde_json::Value, ToolValue represents all the primitive values that
/// tools can produce.
#[derive(Default, Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum ToolValue {
    Text(String),
    AI {
        value: String,
        conversation_id: ConversationId,
    },
    Image(Image),
    FileDiff(FileDiff),
    /// Markdown-formatted text for display in IDE
    Markdown(String),
    /// Paired value: first for LLM (XML), second for display (Markdown)
    Pair(Box<ToolValue>, Box<ToolValue>),
    #[default]
    Empty,
}

impl ToolValue {
    pub fn text(text: String) -> Self {
        ToolValue::Text(text)
    }

    pub fn image(img: Image) -> Self {
        ToolValue::Image(img)
    }

    pub fn pair(llm_value: ToolValue, display_value: ToolValue) -> Self {
        ToolValue::Pair(Box::new(llm_value), Box::new(display_value))
    }

    /// Gets the LLM value from a Pair, or returns self if not a Pair.
    ///
    /// This is used when sending content to the AI model.
    pub fn llm_value(&self) -> &ToolValue {
        match self {
            ToolValue::Pair(llm, _) => llm.as_ref(),
            _ => self,
        }
    }

    /// Gets the display value from a Pair, or returns self if not a Pair.
    ///
    /// This is used when rendering content for IDE/user display.
    pub fn display_value(&self) -> &ToolValue {
        match self {
            ToolValue::Pair(_, display) => display.as_ref(),
            _ => self,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            ToolValue::Text(text) => Some(text),
            ToolValue::Image(_) => None,
            ToolValue::FileDiff(_) => None,
            ToolValue::Empty => None,
            ToolValue::AI { value, .. } => Some(value),
            ToolValue::Markdown(md) => Some(md),
            ToolValue::Pair(_, display) => display.as_str(),
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_success_and_failure_content() {
        let success = ToolResult::new(ToolName::new("test_tool")).success("success message");
        assert!(!success.is_error());
        assert_eq!(success.output.as_str().unwrap(), "success message");

        let failure = ToolResult::new(ToolName::new("test_tool")).failure(
            anyhow::anyhow!("error 1")
                .context("error 2")
                .context("error 3"),
        );
        assert!(failure.is_error());
        insta::assert_snapshot!(failure.output.as_str().unwrap());
    }
}
