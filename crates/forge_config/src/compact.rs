use std::fmt;
use std::ops::Deref;
use std::time::Duration;

use derive_setters::Setters;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A newtype for temperature values with built-in validation
///
/// Temperature controls the randomness in the model's output:
/// - Lower values (e.g., 0.1) make responses more focused, deterministic, and
///   coherent
/// - Higher values (e.g., 0.8) make responses more creative, diverse, and
///   exploratory
/// - Valid range is 0.0 to 2.0
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, JsonSchema)]
pub struct Temperature(f32);

impl Temperature {
    /// Creates a new Temperature value, returning an error if outside the valid
    /// range (0.0 to 2.0)
    pub fn new(value: f32) -> Result<Self, String> {
        if Self::is_valid(value) {
            Ok(Self(value))
        } else {
            Err(format!(
                "temperature must be between 0.0 and 2.0, got {value}"
            ))
        }
    }

    /// Creates a new Temperature value without validation
    ///
    /// # Safety
    /// This function should only be used when the value is known to be valid
    pub fn new_unchecked(value: f32) -> Self {
        debug_assert!(Self::is_valid(value), "invalid temperature: {value}");
        Self(value)
    }

    /// Returns true if the temperature value is within the valid range (0.0 to
    /// 2.0)
    pub fn is_valid(value: f32) -> bool {
        (0.0..=2.0).contains(&value)
    }

    /// Returns the inner f32 value
    pub fn value(&self) -> f32 {
        self.0
    }
}

impl Deref for Temperature {
    type Target = f32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Temperature> for f32 {
    fn from(temp: Temperature) -> Self {
        temp.0
    }
}

impl From<f32> for Temperature {
    fn from(value: f32) -> Self {
        Temperature::new_unchecked(value)
    }
}

impl fmt::Display for Temperature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for Temperature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let formatted = format!("{:.1}", self.0);
        let value = formatted.parse::<f32>().unwrap();
        serializer.serialize_f32(value)
    }
}

impl<'de> Deserialize<'de> for Temperature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        let value = f32::deserialize(deserializer)?;
        if Self::is_valid(value) {
            Ok(Self(value))
        } else {
            Err(Error::custom(format!(
                "temperature must be between 0.0 and 2.0, got {value}"
            )))
        }
    }
}

/// A newtype for top_p values with built-in validation
///
/// Top-p (nucleus sampling) controls the diversity of the model's output:
/// - Lower values (e.g., 0.1) make responses more focused by considering only
///   the most probable tokens
/// - Higher values (e.g., 0.9) make responses more diverse by considering a
///   broader range of tokens
/// - Valid range is 0.0 to 1.0
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, JsonSchema)]
pub struct TopP(f32);

impl TopP {
    /// Creates a new TopP value, returning an error if outside the valid
    /// range (0.0 to 1.0)
    pub fn new(value: f32) -> Result<Self, String> {
        if Self::is_valid(value) {
            Ok(Self(value))
        } else {
            Err(format!("top_p must be between 0.0 and 1.0, got {value}"))
        }
    }

    /// Creates a new TopP value without validation
    ///
    /// # Safety
    /// This function should only be used when the value is known to be valid
    pub fn new_unchecked(value: f32) -> Self {
        debug_assert!(Self::is_valid(value), "invalid top_p: {value}");
        Self(value)
    }

    /// Returns true if the top_p value is within the valid range (0.0 to 1.0)
    pub fn is_valid(value: f32) -> bool {
        (0.0..=1.0).contains(&value)
    }

    /// Returns the inner f32 value
    pub fn value(&self) -> f32 {
        self.0
    }
}

impl Deref for TopP {
    type Target = f32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<TopP> for f32 {
    fn from(top_p: TopP) -> Self {
        top_p.0
    }
}

impl fmt::Display for TopP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for TopP {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let formatted = format!("{:.2}", self.0);
        let value = formatted.parse::<f32>().unwrap();
        serializer.serialize_f32(value)
    }
}

impl<'de> Deserialize<'de> for TopP {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        let value = f32::deserialize(deserializer)?;
        if Self::is_valid(value) {
            Ok(Self(value))
        } else {
            Err(Error::custom(format!(
                "top_p must be between 0.0 and 1.0, got {value}"
            )))
        }
    }
}

/// A newtype for top_k values with built-in validation
///
/// Top-k controls the number of highest probability vocabulary tokens to keep:
/// - Lower values (e.g., 10) make responses more focused by considering only
///   the top K most likely tokens
/// - Higher values (e.g., 100) make responses more diverse by considering more
///   token options
/// - Valid range is 1 to 1000 (inclusive)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, JsonSchema)]
pub struct TopK(u32);

impl TopK {
    /// Creates a new TopK value, returning an error if outside the valid
    /// range (1 to 1000)
    pub fn new(value: u32) -> Result<Self, String> {
        if Self::is_valid(value) {
            Ok(Self(value))
        } else {
            Err(format!("top_k must be between 1 and 1000, got {value}"))
        }
    }

    /// Creates a new TopK value without validation
    ///
    /// # Safety
    /// This function should only be used when the value is known to be valid
    pub fn new_unchecked(value: u32) -> Self {
        debug_assert!(Self::is_valid(value), "invalid top_k: {value}");
        Self(value)
    }

    /// Returns true if the top_k value is within the valid range (1 to 1000)
    pub fn is_valid(value: u32) -> bool {
        (1..=1000).contains(&value)
    }

    /// Returns the inner u32 value
    pub fn value(&self) -> u32 {
        self.0
    }
}

impl Deref for TopK {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<TopK> for u32 {
    fn from(top_k: TopK) -> Self {
        top_k.0
    }
}

impl fmt::Display for TopK {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for TopK {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(self.0)
    }
}

impl<'de> Deserialize<'de> for TopK {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        let value = u32::deserialize(deserializer)?;
        if Self::is_valid(value) {
            Ok(Self(value))
        } else {
            Err(Error::custom(format!(
                "top_k must be between 1 and 1000, got {value}"
            )))
        }
    }
}

/// A newtype for max_tokens values with built-in validation
///
/// Max tokens controls the maximum number of tokens the model can generate:
/// - Lower values (e.g., 100) limit response length for concise outputs
/// - Higher values (e.g., 4000) allow for longer, more detailed responses
/// - Valid range is 1 to 100,000 (reasonable upper bound for most models)
/// - If not specified, the model provider's default will be used
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, JsonSchema)]
pub struct MaxTokens(u32);

impl MaxTokens {
    /// Creates a new MaxTokens value, returning an error if outside the valid
    /// range (1 to 100,000)
    pub fn new(value: u32) -> Result<Self, String> {
        if Self::is_valid(value) {
            Ok(Self(value))
        } else {
            Err(format!(
                "max_tokens must be between 1 and 100000, got {value}"
            ))
        }
    }

    /// Creates a new MaxTokens value without validation
    ///
    /// # Safety
    /// This function should only be used when the value is known to be valid
    pub fn new_unchecked(value: u32) -> Self {
        debug_assert!(Self::is_valid(value), "invalid max_tokens: {value}");
        Self(value)
    }

    /// Returns true if the max_tokens value is within the valid range (1 to
    /// 100,000)
    pub fn is_valid(value: u32) -> bool {
        (1..=100_000).contains(&value)
    }

    /// Returns the inner u32 value
    pub fn value(&self) -> u32 {
        self.0
    }
}

impl Deref for MaxTokens {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<MaxTokens> for u32 {
    fn from(max_tokens: MaxTokens) -> Self {
        max_tokens.0
    }
}

impl fmt::Display for MaxTokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for MaxTokens {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(self.0)
    }
}

impl<'de> Deserialize<'de> for MaxTokens {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        let value = u32::deserialize(deserializer)?;
        if Self::is_valid(value) {
            Ok(Self(value))
        } else {
            Err(Error::custom(format!(
                "max_tokens must be between 1 and 100000, got {value}"
            )))
        }
    }
}

/// Frequency at which forge checks for updates
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateFrequency {
    Daily,
    Weekly,
    #[default]
    Always,
}

impl From<UpdateFrequency> for Duration {
    fn from(val: UpdateFrequency) -> Self {
        match val {
            UpdateFrequency::Daily => Duration::from_secs(60 * 60 * 24),
            UpdateFrequency::Weekly => Duration::from_secs(60 * 60 * 24 * 7),
            UpdateFrequency::Always => Duration::ZERO,
        }
    }
}

/// Configuration for automatic forge updates
#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema, Setters, PartialEq)]
#[setters(strip_option, into)]
pub struct Update {
    /// How frequently forge checks for updates
    pub frequency: Option<UpdateFrequency>,
    /// Whether to automatically install updates without prompting
    pub auto_update: Option<bool>,
}

fn deserialize_percentage<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;

    let value = f64::deserialize(deserializer)?;
    if !(0.0..=1.0).contains(&value) {
        return Err(Error::custom(format!(
            "percentage must be between 0.0 and 1.0, got {value}"
        )));
    }
    Ok(value)
}

/// Optional tag name used when extracting summarized content during compaction
#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, PartialEq)]
#[serde(transparent)]
pub struct SummaryTag(String);

impl Default for SummaryTag {
    fn default() -> Self {
        SummaryTag("forge_context_summary".to_string())
    }
}

impl SummaryTag {
    /// Returns the inner string slice
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Configuration for automatic context compaction for all agents
#[derive(Debug, Clone, Serialize, Deserialize, Setters, JsonSchema, PartialEq)]
#[setters(strip_option, into)]
pub struct Compact {
    /// Number of most recent messages to preserve during compaction.
    /// These messages won't be considered for summarization. Works alongside
    /// eviction_window - the more conservative limit (fewer messages to
    /// compact) takes precedence.
    #[serde(default)]
    pub retention_window: usize,

    /// Maximum percentage of the context that can be summarized during
    /// compaction. Valid values are between 0.0 and 1.0, where 0.0 means no
    /// compaction and 1.0 allows summarizing all messages. Works alongside
    /// retention_window - the more conservative limit (fewer messages to
    /// compact) takes precedence.
    #[serde(default, deserialize_with = "deserialize_percentage")]
    pub eviction_window: f64,

    /// Maximum number of tokens to keep after compaction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,

    /// Maximum number of tokens before triggering compaction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_threshold: Option<usize>,

    /// Maximum number of conversation turns before triggering compaction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_threshold: Option<usize>,

    /// Maximum number of messages before triggering compaction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_threshold: Option<usize>,

    /// Model ID to use for compaction, useful when compacting with a
    /// cheaper/faster model. If not specified, the root level model will be
    /// used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Optional tag name to extract content from when summarizing (e.g.,
    /// "summary")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_tag: Option<SummaryTag>,

    /// Whether to trigger compaction when the last message is from a user
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_turn_end: Option<bool>,
}

impl Default for Compact {
    fn default() -> Self {
        Self::new()
    }
}

impl Compact {
    /// Creates a new compaction configuration with all optional fields unset
    pub fn new() -> Self {
        Self {
            max_tokens: None,
            token_threshold: None,
            turn_threshold: None,
            message_threshold: None,
            summary_tag: None,
            model: None,
            eviction_window: 0.2,
            retention_window: 0,
            on_turn_end: None,
        }
    }
}
