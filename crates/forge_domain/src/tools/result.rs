use derive_setters::Setters;
use forge_template::Element;
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
                    Element::new("tool_call_error")
                        .append(Element::new("cause").cdata(message.join("\n")))
                        .append(Element::new("reflection").text(REFLECTION_PROMPT)),
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

    pub fn markdown(md: impl ToString) -> Self {
        ToolOutput {
            is_error: Default::default(),
            values: vec![ToolValue::Markdown(md.to_string())],
        }
    }

    /// Creates a paired output with XML for LLM and Markdown for display.
    /// This is the recommended way to create outputs that need different
    /// representations for AI and human consumption.
    ///
    /// # Example
    /// ```ignore
    /// use forge_template::Element;
    ///
    /// let elm = Element::new("search_results")
    ///     .attr("pattern", "*.rs")
    ///     .text("file.rs");
    ///
    /// let output = ToolOutput::paired(elm.render(), elm.render_markdown());
    /// ```
    pub fn paired(xml: impl ToString, markdown: impl ToString) -> Self {
        ToolOutput {
            is_error: Default::default(),
            values: vec![
                ToolValue::Text(xml.to_string()).pair(ToolValue::Markdown(markdown.to_string())),
            ],
        }
    }

    /// Creates a paired output from an Element and Markdown builder.
    /// The Element provides XML for LLM, the Markdown provides display format.
    ///
    /// # Example
    /// ```ignore
    /// use forge_template::{Element, Markdown};
    ///
    /// let elm = Element::new("search_results")
    ///     .attr("pattern", "*.rs")
    ///     .text("file.rs");
    ///
    /// let md = Markdown::new()
    ///     .bold("Search Results")
    ///     .kv_code("Pattern", "*.rs");
    ///
    /// let output = ToolOutput::from_element_and_markdown(elm, md);
    /// ```
    pub fn from_element_and_markdown(element: Element, markdown: forge_template::Markdown) -> Self {
        Self::paired(element.render(), markdown.render())
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

    pub fn combine_mut(&mut self, value: ToolOutput) {
        self.values.extend(value.values);
    }

    pub fn combine(self, other: ToolOutput) -> Self {
        let mut items = self.values;
        items.extend(other.values);
        ToolOutput { values: items, is_error: self.is_error || other.is_error }
    }

    /// Returns the first item as a string if it exists (uses LLM value for
    /// Pairs)
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

/// Represents a file modification with before and after content.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    pub old_text: Option<String>,
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
    #[default]
    Empty,
    Markdown(String),
    Pair(Box<ToolValue>, Box<ToolValue>),
}

impl ToolValue {
    pub fn text(text: String) -> Self {
        ToolValue::Text(text)
    }

    pub fn markdown(md: String) -> Self {
        ToolValue::Markdown(md)
    }

    pub fn image(img: Image) -> Self {
        ToolValue::Image(img)
    }

    /// Pairs this value with another, creating a `Pair` variant.
    /// Typically used to pair XML content (for LLM) with Markdown (for
    /// display).
    ///
    /// # Example
    /// ```ignore
    /// let xml = ToolValue::text(element.render());
    /// let md = ToolValue::markdown(element.render_markdown());
    /// let paired = xml.pair(md);
    /// ```
    pub fn pair(self, other: impl Into<ToolValue>) -> Self {
        ToolValue::Pair(Box::new(self), Box::new(other.into()))
    }

    /// Gets the LLM value from a Pair, or returns self if not a Pair.
    /// The LLM value is the first value in the pair (typically XML-formatted).
    pub fn llm_value(&self) -> &ToolValue {
        match self {
            ToolValue::Pair(llm, _) => llm.as_ref(),
            _ => self,
        }
    }

    /// Gets the display value from a Pair, or returns self if not a Pair.
    /// The display value is the second value in the pair (typically
    /// Markdown-formatted).
    pub fn display_value(&self) -> &ToolValue {
        match self {
            ToolValue::Pair(_, display) => display.as_ref(),
            _ => self,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            ToolValue::Text(text) => Some(text),
            ToolValue::Markdown(md) => Some(md),
            ToolValue::Image(_) => None,
            ToolValue::Empty => None,
            ToolValue::AI { value, .. } => Some(value),
            ToolValue::FileDiff(_) => None,
            ToolValue::Pair(llm, _) => llm.as_str(),
        }
    }

    /// Gets the string representation suitable for display (UI, terminal,
    /// etc.). For Pair values, returns the display value; otherwise returns
    /// as_str().
    pub fn display_str(&self) -> Option<&str> {
        match self {
            ToolValue::Pair(_, display) => display.as_str(),
            _ => self.as_str(),
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
