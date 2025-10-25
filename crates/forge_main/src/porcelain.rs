use std::collections::HashMap;
use std::fmt;

use indexmap::IndexSet;

use crate::info::{Info, Section};

/// Porcelain is an intermediate representation that converts Info into a flat,
/// tabular structure suitable for machine-readable output.
///
/// Structure: Vec<(String, Vec<Option<String>>)>
/// - First element: Section name
/// - Second element: Vec of Option<String> pairs where:
///   - Index 0, 2, 4... are keys
///   - Index 1, 3, 5... are values
///   - None = missing value
#[derive(Debug, PartialEq)]
pub struct Porcelain(Vec<Vec<Option<String>>>);

impl Porcelain {
    /// Creates a new empty Porcelain instance
    pub fn new() -> Self {
        Porcelain(Vec::new())
    }

    /// Skips the first n rows
    pub fn skip_row(self, n: usize) -> Self {
        Porcelain(self.0.into_iter().skip(n).collect())
    }

    pub fn drop_col(self, c: usize) -> Self {
        Porcelain(
            self.0
                .into_iter()
                .map(|row| {
                    row.into_iter()
                        .enumerate()
                        .filter_map(|(i, col)| if i == c { None } else { Some(col) })
                        .collect()
                })
                .collect(),
        )
    }

    pub fn into_body(self) -> Vec<Vec<Option<String>>> {
        // Skip headers and return
        self.0.into_iter().skip(1).collect()
    }

    pub fn into_rows(self) -> Vec<Vec<Option<String>>> {
        self.0
    }

    pub fn transpose(self) -> Self {
        self
    }
}

impl fmt::Display for Porcelain {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

impl Default for Porcelain {
    fn default() -> Self {
        Self::new()
    }
}

/// Converts Info to Porcelain representation
/// Handles both cases:
/// - Info with titles: Each title becomes a row with its associated items
/// - Info without titles: Each item becomes its own row
impl From<Info> for Porcelain {
    fn from(info: Info) -> Self {
        Porcelain::from(&info)
    }
}

/// Converts Info reference to Porcelain representation
impl From<&Info> for Porcelain {
    fn from(info: &Info) -> Self {
        let mut rows = Vec::new();
        let mut cells = HashMap::new();
        let mut in_row = false;
        // Extract all unique keys
        let mut keys = IndexSet::new();

        for section in info.sections() {
            match section {
                Section::Title(title) => {
                    if in_row {
                        rows.push(cells.clone());
                        cells = HashMap::new();
                    }

                    in_row = true;
                    cells.insert("$ID", Some(title.to_owned()));
                    keys.insert("$ID");
                }
                Section::Items(key, value) => {
                    cells.insert(key, value.clone());
                    keys.insert(key);
                }
            }
        }

        if in_row {
            rows.push(cells.clone());
        }

        // Insert Headers
        let mut data = vec![
            keys.iter()
                .map(|head| Some((*head).to_owned()))
                .collect::<Vec<_>>(),
        ];

        // Insert Rows
        data.extend(rows.iter().map(|rows| {
            keys.iter()
                .map(|key| rows.get(*key).and_then(|value| value.as_ref().cloned()))
                .collect::<Vec<Option<String>>>()
        }));
        Porcelain(data)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_from_info() {
        let info = Info::new()
            .add_title("user1")
            .add_key_value("name", "Alice")
            .add_key_value("age", "30")
            .add_title("user2")
            .add_key_value("name", "Bob")
            .add_key_value("age", "25");

        let actual = Porcelain::from(info).into_body();
        let expected = vec![
            vec![
                Some("user1".into()),
                Some("Alice".into()),
                Some("30".into()),
            ],
            vec![Some("user2".into()), Some("Bob".into()), Some("25".into())],
        ];

        assert_eq!(actual, expected)
    }

    #[test]
    fn test_from_unordered_info() {
        let info = Info::new()
            .add_title("user1")
            .add_key_value("name", "Alice")
            .add_key_value("age", "30")
            .add_title("user2")
            .add_key_value("age", "25")
            .add_key_value("name", "Bob");

        let actual = Porcelain::from(info).into_body();
        let expected = vec![
            vec![
                Some("user1".into()),
                Some("Alice".into()),
                Some("30".into()),
            ],
            vec![Some("user2".into()), Some("Bob".into()), Some("25".into())],
        ];

        assert_eq!(actual, expected)
    }

    #[test]
    fn test_drop_col() {
        let info = Porcelain(vec![
            vec![
                Some("user1".into()),
                Some("Alice".into()),
                Some("30".into()),
            ],
            vec![Some("user2".into()), Some("Bob".into()), Some("25".into())],
        ]);

        let actual = dbg!(info.drop_col(1).into_rows());

        let expected = vec![
            vec![Some("user1".into()), Some("30".into())],
            vec![Some("user2".into()), Some("25".into())],
        ];

        assert_eq!(actual, expected)
    }

    #[test]
    fn test_transpose() {
        let info = Info::new()
            .add_title("env")
            .add_key_value("version", "0.1.0")
            .add_key_value("shell", "zsh")
            .add_title("conversation")
            .add_key_value("id", "000-000-000")
            .add_key_value("title", "make agents great again")
            .add_title("agent")
            .add_key_value("id", "forge")
            .add_key_value("model", "sonnet-4");

        let actual = Porcelain::from(info).transpose().into_body();
        let expected = vec![
            vec![
                Some("env".into()),
                Some("version".into()),
                Some("0.1.0".into()),
            ],
            vec![Some("env".into()), Some("shell".into()), Some("zsh".into())],
        ];

        assert_eq!(actual, expected)
    }

    //     #[test]
    //     fn test_porcelain_conversion() {
    //         // Test converting Info to Porcelain
    //         let fixture = Info::new()
    //             .add_title("user1")
    //             .add_key_value("name", "Alice")
    //             .add_key_value("age", "30")
    //             .add_title("user2")
    //             .add_key_value("name", "Bob")
    //             .add_key_value("age", "25");

    //         let actual: Porcelain = fixture.into();

    //         // Verify structure: sections with alternating key-value pairs
    //         let expected = Porcelain::new()
    //             .add_section(
    //                 "user1".to_string(),
    //                 vec![
    //                     Some("name".to_string()),
    //                     Some("Alice".to_string()),
    //                     Some("age".to_string()),
    //                     Some("30".to_string()),
    //                 ],
    //             )
    //             .add_section(
    //                 "user2".to_string(),
    //                 vec![
    //                     Some("name".to_string()),
    //                     Some("Bob".to_string()),
    //                     Some("age".to_string()),
    //                     Some("25".to_string()),
    //                 ],
    //             );

    //         assert_eq!(actual, expected);
    //     }

    //     #[test]
    //     fn test_porcelain_to_rows() {
    //         // Test converting Porcelain to rows
    //         let fixture = Porcelain::new()
    //             .add_section(
    //                 "user1".to_string(),
    //                 vec![
    //                     Some("name".to_string()),
    //                     Some("Alice".to_string()),
    //                     Some("age".to_string()),
    //                     Some("30".to_string()),
    //                 ],
    //             )
    //             .add_section(
    //                 "user2".to_string(),
    //                 vec![
    //                     Some("name".to_string()),
    //                     Some("Bob".to_string()),
    //                     Some("age".to_string()),
    //                     Some("25".to_string()),
    //                 ],
    //             );

    //         let actual = fixture.to_rows();

    //         let expected = vec![
    //             vec!["user1".to_string(), "Alice".to_string(),
    // "30".to_string()],             vec!["user2".to_string(),
    // "Bob".to_string(), "25".to_string()],         ];

    //         assert_eq!(actual, expected);
    //     }

    //     #[test]
    //     fn test_porcelain_with_missing_values() {
    //         // Test Porcelain with missing values (None)
    //         let fixture = Info::new()
    //             .add_title("user1")
    //             .add_key_value("name", "Alice")
    //             .add_key_value("age", "30")
    //             .add_title("user2")
    //             .add_key_value("age", "25"); // Missing name

    //         let porcelain: Porcelain = fixture.into();
    //         let actual = porcelain.to_rows();

    //         // user2 should have empty string for missing name
    //         let expected = vec![
    //             vec!["user1".to_string(), "Alice".to_string(),
    // "30".to_string()],             vec!["user2".to_string(),
    // "".to_string(), "25".to_string()],         ];

    //         assert_eq!(actual, expected);
    //     }

    //     #[test]
    //     fn test_porcelain_with_different_field_order() {
    //         // Test that Porcelain maintains consistent field order across
    // sections         let fixture = Info::new()
    //             .add_title("user1")
    //             .add_key_value("name", "Alice")
    //             .add_key_value("age", "30")
    //             .add_key_value("city", "NYC")
    //             .add_title("user2")
    //             .add_key_value("age", "25") // Different order
    //             .add_key_value("name", "Bob");

    //         let porcelain: Porcelain = fixture.into();
    //         let actual = porcelain.to_rows();

    //         // All rows should have same column order: [title, name, age,
    // city]         let expected = vec![
    //             vec![
    //                 "user1".to_string(),
    //                 "Alice".to_string(),
    //                 "30".to_string(),
    //                 "NYC".to_string(),
    //             ],
    //             vec![
    //                 "user2".to_string(),
    //                 "Bob".to_string(),
    //                 "25".to_string(),
    //                 "".to_string(),
    //             ],
    //         ];

    //         assert_eq!(actual, expected);
    //     }

    //     #[test]
    //     fn test_porcelain_empty() {
    //         // Test empty Porcelain
    //         let fixture = Porcelain::new();

    //         let actual = fixture.to_rows();
    //         let expected: Vec<Vec<String>> = vec![];

    //         assert_eq!(actual, expected);
    //     }

    //     #[test]
    //     fn test_porcelain_skip() {
    //         // Test skipping sections using Info structure (flat format)
    //         let info = Info::new()
    //             .add_key_value("section1", "")
    //             .add_key_value("section2", "Alice")
    //             .add_key_value("section3", "30");

    //         let porcelain = Porcelain::from(&info);
    //         let actual = porcelain.skip_row(1).to_rows();

    //         // Should skip section1
    //         let expected = vec![
    //             vec!["section2".to_string(), "Alice".to_string()],
    //             vec!["section3".to_string(), "30".to_string()],
    //         ];

    //         assert_eq!(actual, expected);
    //     }

    //     #[test]
    //     fn test_porcelain_skip_more_than_available() {
    //         // Test skipping more sections than available using Info
    // structure (flat format)         let info =
    // Info::new().add_key_value("section1", "Alice");

    //         let porcelain = Porcelain::from(&info);
    //         let actual = porcelain.skip_row(5).to_rows(); // Skip more than
    // exists

    //         // Should return empty since we skip more than available
    //         let expected: Vec<Vec<String>> = vec![];

    //         assert_eq!(actual, expected);
    //     }

    //     #[test]
    //     fn test_info_to_rows() {
    //         // Test converting Info to rows without titles
    //         let fixture = Info::new()
    //             .add_key_value("name", "Alice")
    //             .add_key_value("age", "30")
    //             .add_key("city"); // Key-only item

    //         let actual = Porcelain::from(&fixture).to_rows();

    //         let expected = vec![
    //             vec!["name".to_string(), "Alice".to_string()],
    //             vec!["age".to_string(), "30".to_string()],
    //             vec!["city".to_string()],
    //         ];

    //         assert_eq!(actual, expected);
    //     }

    //     #[test]
    //     fn test_info_command_structure() {
    //         // Test the actual structure of the info command
    //         // Multiple titles with different schemas - should be flat
    //         let fixture = Info::new()
    //             .add_title("PATHS")
    //             .add_key_value("Logs", "~/forge/logs")
    //             .add_key_value("Agents", "~/forge/agents")
    //             .add_key_value("History", "~/forge/.forge_history")
    //             .add_key_value("Checkpoints", "~/forge/snapshots")
    //             .add_key_value("Policies", "~/forge/permissions.yaml")
    //             .add_title("ENVIRONMENT")
    //             .add_key_value("Version", "0.1.0")
    //             .add_key_value("Working Directory", "~/code-forge")
    //             .add_key_value("Shell", "/bin/zsh")
    //             .add_key_value("Git Branch", "main")
    //             .add_title("CONVERSATION")
    //             .add_key_value("ID", "f266080c-fec6-426b-914e-178acc39483f")
    //             .add_title("TOKEN USAGE")
    //             .add_key_value("Total", "49,701")
    //             .add_key_value("Input", "49,440")
    //             .add_key_value("Cached", "116 [99%]")
    //             .add_key_value("Output", "261")
    //             .add_key_value("Cost", "$0.0176")
    //             .add_title("AGENT")
    //             .add_key_value("Name", "FORGE")
    //             .add_key_value("Provider", "anthropic")
    //             .add_key_value("Model", "claude-sonnet-4.5")
    //             .add_key_value("Endpoint", "https://openrouter.ai/api/v1/chat/completions")
    //             .add_key_value("API Key", "sk-or-v1-31eb...4631");

    //         let actual = Porcelain::from(&fixture).to_rows();

    //         // Should be flat key-value pairs without category headers
    //         let expected = vec![
    //             vec!["Logs".to_string(), "~/forge/logs".to_string()],
    //             vec!["Agents".to_string(), "~/forge/agents".to_string()],
    //             vec!["History".to_string(),
    // "~/forge/.forge_history".to_string()],
    // vec!["Checkpoints".to_string(), "~/forge/snapshots".to_string()],
    //             vec![
    //                 "Policies".to_string(),
    //                 "~/forge/permissions.yaml".to_string(),
    //             ],
    //             vec!["Version".to_string(), "0.1.0".to_string()],
    //             vec!["Working Directory".to_string(),
    // "~/code-forge".to_string()],             vec!["Shell".to_string(),
    // "/bin/zsh".to_string()],             vec!["Git Branch".to_string(),
    // "main".to_string()],             vec![
    //                 "ID".to_string(),
    //                 "f266080c-fec6-426b-914e-178acc39483f".to_string(),
    //             ],
    //             vec!["Total".to_string(), "49,701".to_string()],
    //             vec!["Input".to_string(), "49,440".to_string()],
    //             vec!["Cached".to_string(), "116 [99%]".to_string()],
    //             vec!["Output".to_string(), "261".to_string()],
    //             vec!["Cost".to_string(), "$0.0176".to_string()],
    //             vec!["Name".to_string(), "FORGE".to_string()],
    //             vec!["Provider".to_string(), "anthropic".to_string()],
    //             vec!["Model".to_string(), "claude-sonnet-4.5".to_string()],
    //             vec![
    //                 "Endpoint".to_string(),
    //                 "https://openrouter.ai/api/v1/chat/completions".to_string(),
    //             ],
    //             vec!["API Key".to_string(),
    // "sk-or-v1-31eb...4631".to_string()],         ];

    //         assert_eq!(actual, expected);
    //     }

    //     #[test]
    //     fn test_tools_command_structure() {
    //         // Test the actual structure of the tools command
    //         // Multiple titles (categories) with many key-only items - should
    // be flat         let fixture = Info::new()
    //             .add_title("TOOLS")
    //             .add_key("[✓] read")
    //             .add_key("[✓] write")
    //             .add_key("[✓] search")
    //             .add_key("[✓] remove")
    //             .add_key("[✓] patch")
    //             .add_key("[✓] undo")
    //             .add_key("[✓] shell")
    //             .add_key("[✓] fetch")
    //             .add_key("[ ] followup")
    //             .add_key("[ ] plan")
    //             .add_key("[ ] muse")
    //             .add_key("[ ] forge")
    //             .add_key("[✓] sage")
    //             .add_title("MCP TOOLS")
    //             .add_key("[✓] mcp_deepwiki_tool_read_wiki_contents")
    //             .add_key("[✓] mcp_deepwiki_tool_ask_question")
    //             .add_key("[✓] mcp_context7_tool_get_library_docs");

    //         let actual = Porcelain::from(&fixture).to_rows();

    //         // Should be flat list of tools without category headers
    //         let expected = vec![
    //             vec!["[✓] read".to_string()],
    //             vec!["[✓] write".to_string()],
    //             vec!["[✓] search".to_string()],
    //             vec!["[✓] remove".to_string()],
    //             vec!["[✓] patch".to_string()],
    //             vec!["[✓] undo".to_string()],
    //             vec!["[✓] shell".to_string()],
    //             vec!["[✓] fetch".to_string()],
    //             vec!["[ ] followup".to_string()],
    //             vec!["[ ] plan".to_string()],
    //             vec!["[ ] muse".to_string()],
    //             vec!["[ ] forge".to_string()],
    //             vec!["[✓] sage".to_string()],
    //             vec!["[✓] mcp_deepwiki_tool_read_wiki_contents".to_string()],
    //             vec!["[✓] mcp_deepwiki_tool_ask_question".to_string()],
    //             vec!["[✓] mcp_context7_tool_get_library_docs".to_string()],
    //         ];

    //         assert_eq!(actual, expected);
    //     }

    //     #[test]
    //     fn test_single_provider_structure() {
    //         // Test single provider (should be hierarchical even with 1
    // section)         let fixture = Info::new()
    //             .add_title("PROVIDERS")
    //             .add_title("OpenRouter")
    //             .add_key_value("Domain", "[openrouter.ai]");

    //         let actual = Porcelain::from(&fixture).skip_row(1).to_rows();

    //         // Should be hierarchical table with provider name as first
    // column         let expected = vec![vec![
    //             "OpenRouter".to_string(),
    //             "[openrouter.ai]".to_string(),
    //         ]];

    //         assert_eq!(actual, expected);
    //     }

    //     #[test]
    //     fn test_multiple_providers_structure() {
    //         // Test multiple providers (hierarchical table)
    //         let fixture = Info::new()
    //             .add_title("PROVIDERS")
    //             .add_title("OpenRouter")
    //             .add_key_value("Domain", "[openrouter.ai]")
    //             .add_title("Anthropic")
    //             .add_key_value("Domain", "[api.anthropic.com]");

    //         let actual = Porcelain::from(&fixture).skip_row(1).to_rows();

    //         // Should be hierarchical table
    //         let expected = vec![
    //             vec!["OpenRouter".to_string(),
    // "[openrouter.ai]".to_string()],
    // vec!["Anthropic".to_string(), "[api.anthropic.com]".to_string()],
    //         ];

    //         assert_eq!(actual, expected);
    //     }

    //     #[test]
    //     fn test_porcelain_display_empty() {
    //         // Test Display trait with empty Porcelain
    //         let fixture = Porcelain::new();

    //         let actual = fixture.to_string();
    //         let expected = "";

    //         assert_eq!(actual, expected);
    //     }

    //     #[test]
    //     fn test_porcelain_display_hierarchical() {
    //         // Test Display trait with hierarchical data
    //         let fixture = Porcelain::new()
    //             .add_section(
    //                 "user1".to_string(),
    //                 vec![
    //                     Some("name".to_string()),
    //                     Some("Alice".to_string()),
    //                     Some("age".to_string()),
    //                     Some("30".to_string()),
    //                 ],
    //             )
    //             .add_section(
    //                 "user2".to_string(),
    //                 vec![
    //                     Some("name".to_string()),
    //                     Some("Bob".to_string()),
    //                     Some("age".to_string()),
    //                     Some("25".to_string()),
    //                 ],
    //             );

    //         let actual = fixture.to_string();
    //         let expected = "user1 Alice 30\nuser2 Bob   25";

    //         assert_eq!(actual, expected);
    //     }

    //     #[test]
    //     fn test_porcelain_display_flat() {
    //         // Test Display trait with flat key-value data
    //         let fixture = Porcelain::new()
    //             .add_section(
    //                 String::new(),
    //                 vec![Some("name".to_string()),
    // Some("Alice".to_string())],             )
    //             .add_section(
    //                 String::new(),
    //                 vec![Some("age".to_string()), Some("30".to_string())],
    //             )
    //             .add_section(String::new(), vec![Some("city".to_string())]);

    //         let actual = fixture.to_string();
    //         let expected = "name Alice\nage  30\ncity";

    //         assert_eq!(actual, expected);
    //     }

    //     #[test]
    //     fn test_porcelain_display_missing_values() {
    //         // Test Display trait with missing values
    //         let fixture = Porcelain::new()
    //             .add_section(
    //                 "user1".to_string(),
    //                 vec![
    //                     Some("name".to_string()),
    //                     Some("Alice".to_string()),
    //                     Some("age".to_string()),
    //                     Some("30".to_string()),
    //                 ],
    //             )
    //             .add_section(
    //                 "user2".to_string(),
    //                 vec![
    //                     Some("name".to_string()),
    //                     None, // Missing name
    //                     Some("age".to_string()),
    //                     Some("25".to_string()),
    //                 ],
    //             );

    //         let actual = fixture.to_string();
    //         let expected = "user1 Alice 30\nuser2       25";

    //         assert_eq!(actual, expected);
    //     }

    //     #[test]
    //     fn test_porcelain_display_from_info() {
    //         // Test Display trait with real Info data
    //         let info = Info::new()
    //             .add_title("PROVIDERS")
    //             .add_title("OpenRouter")
    //             .add_key_value("Domain", "[openrouter.ai]")
    //             .add_title("Anthropic")
    //             .add_key_value("Domain", "[api.anthropic.com]");

    //         let porcelain = Porcelain::from(&info).skip_row(1);
    //         let actual = porcelain.to_string();
    //         let expected = "OpenRouter [openrouter.ai]\nAnthropic
    // [api.anthropic.com]";

    //         assert_eq!(actual, expected);
    //     }
}
