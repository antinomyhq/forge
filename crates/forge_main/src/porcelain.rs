use std::collections::HashMap;

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
///   - Two consecutive Nones (None, None) = no item in section
#[derive(Debug, PartialEq)]
pub struct Porcelain(Vec<(String, Vec<Option<String>>)>);

impl Porcelain {
    /// Creates a new empty Porcelain instance
    pub fn new() -> Self {
        Porcelain(Vec::new())
    }

    /// Adds a section with key-value pairs
    pub fn add_section(mut self, title: String, items: Vec<Option<String>>) -> Self {
        self.0.push((title, items));
        self
    }

    /// Converts Porcelain to rows for display
    /// Each section becomes a row with [title, value1, value2, ...] if title
    /// exists Or [value1, value2, ...] if title is empty (for simple
    /// key-value Info)
    pub fn to_rows(&self) -> Vec<Vec<String>> {
        let mut rows = Vec::new();

        for (title, items) in &self.0 {
            let mut row = Vec::new();

            // Only add title if it's not empty
            if !title.is_empty() {
                row.push(title.clone());
            }

            // Extract values (odd indices) from the items, skipping keys (even indices)
            for (idx, item) in items.iter().enumerate() {
                if idx % 2 == 1 {
                    // Odd index = value
                    row.push(item.clone().unwrap_or_default());
                } else if title.is_empty() && idx % 2 == 0 {
                    // Even index (key) - add it for simple key-value Info without titles
                    row.push(item.clone().unwrap_or_default());
                }
            }

            rows.push(row);
        }

        rows
    }

    /// Checks if a section has no items (consecutive None, None)
    fn _has_no_items(items: &[Option<String>]) -> bool {
        items.windows(2).any(|w| w[0].is_none() && w[1].is_none())
    }

    /// Skips the first n sections
    /// Useful for porcelain mode where top-level titles should not be displayed
    pub fn skip(mut self, n: usize) -> Self {
        if n > 0 && n <= self.0.len() {
            self.0.drain(0..n);
        }
        self
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
        // Analyze the Info structure to determine how to convert it
        let mut title_count = 0;
        let mut key_value_count = 0;
        let mut key_only_count = 0;

        // Track field names per section to detect if sections share the same schema
        let mut sections_fields: Vec<std::collections::HashSet<String>> = Vec::new();
        let mut current_fields = std::collections::HashSet::new();

        for section in info.sections() {
            match section {
                Section::Title(_) => {
                    title_count += 1;
                    // Save previous section's field names
                    if !current_fields.is_empty() {
                        sections_fields.push(current_fields.clone());
                        current_fields.clear();
                    }
                }
                Section::Items(key, value) => {
                    current_fields.insert(key.clone());
                    if value.is_some() {
                        key_value_count += 1;
                    } else {
                        key_only_count += 1;
                    }
                }
            }
        }
        // Don't forget the last section
        if !current_fields.is_empty() {
            sections_fields.push(current_fields);
        }

        // Check if sections have overlapping fields (same schema = table rows)
        // If sections have completely different fields, they're categories (flat)
        let has_field_overlap = if sections_fields.len() > 1 {
            // Multiple sections: check for common fields across all
            let first_section = &sections_fields[0];
            let common_fields: std::collections::HashSet<_> = first_section
                .iter()
                .filter(|field| sections_fields.iter().skip(1).all(|s| s.contains(*field)))
                .collect();

            // If there are ANY common fields across all sections, it's likely a table
            !common_fields.is_empty()
        } else if sections_fields.len() == 1 {
            // Single section with multiple titles (e.g., single provider/agent)
            // Assume hierarchical if structure looks like: Title + EntityTitle + fields
            true
        } else {
            // No sections with fields
            false
        };

        // Hierarchical table structure:
        // - Multiple titles with overlapping field names (same schema = table rows)
        // - OR single entity with title structure (e.g., one provider/agent)
        // - Primarily key-value pairs (not key-only lists like tools)
        //
        // Flat list structure:
        // - Single title OR no field overlap (different schemas = categories)
        // - HashMap handles field ordering automatically
        let is_hierarchical_table = title_count > 1
            && key_value_count > 0
            && key_value_count >= key_only_count
            && has_field_overlap;

        if !is_hierarchical_table {
            // Simple flat list (possibly with categories to skip)
            // Cases:
            // - config (1 title + items)
            // - tools (multiple titles + key-only items)
            // - info (multiple titles + inconsistent schemas)
            let mut porcelain = Porcelain::new();
            for section in info.sections() {
                if let Section::Items(key, value) = section {
                    let items = if let Some(value_str) = value {
                        // Key-value pair
                        vec![Some(key.clone()), Some(value_str.clone())]
                    } else {
                        // Key-only item
                        vec![Some(key.clone())]
                    };
                    porcelain = porcelain.add_section(String::new(), items);
                }
            }
            return porcelain;
        }

        // Hierarchical table case: flatten structure into rows
        // First pass: collect all unique field names in order
        let mut field_order = Vec::new();
        let mut seen_fields = std::collections::HashSet::new();

        for section in info.sections() {
            if let Section::Items(key, _) = section
                && !seen_fields.contains(key)
            {
                field_order.push(key.clone());
                seen_fields.insert(key.clone());
            }
        }

        // Second pass: build Porcelain structure
        let mut porcelain = Porcelain::new();
        let mut current_title = String::new();
        let mut current_row_data: HashMap<String, String> = HashMap::new();

        for section in info.sections() {
            match section {
                Section::Title(title) => {
                    // If we have data, build section for previous title
                    if !current_title.is_empty() {
                        let items = build_porcelain_items(&current_row_data, &field_order);
                        porcelain = porcelain.add_section(current_title.clone(), items);
                        current_row_data.clear();
                    }
                    current_title = title.clone();
                }
                Section::Items(key, value) => {
                    let value_str = value.clone().unwrap_or_default();
                    current_row_data.insert(key.clone(), value_str);
                }
            }
        }

        // Push the last section
        if !current_title.is_empty() {
            let items = build_porcelain_items(&current_row_data, &field_order);
            porcelain = porcelain.add_section(current_title, items);
        }

        porcelain
    }
}

/// Helper function to build items vector for Porcelain
/// Creates alternating key-value pairs: [Some(key1), Some(value1), Some(key2),
/// Some(value2), ...] Missing fields are represented as [Some(key), None]
fn build_porcelain_items(
    row_data: &HashMap<String, String>,
    field_order: &[String],
) -> Vec<Option<String>> {
    let mut items = Vec::new();

    for field in field_order {
        items.push(Some(field.clone())); // Key
        items.push(row_data.get(field).cloned()); // Value (None if missing)
    }

    items
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_porcelain_conversion() {
        // Test converting Info to Porcelain
        let fixture = Info::new()
            .add_title("user1")
            .add_key_value("name", "Alice")
            .add_key_value("age", "30")
            .add_title("user2")
            .add_key_value("name", "Bob")
            .add_key_value("age", "25");

        let actual: Porcelain = fixture.into();

        // Verify structure: sections with alternating key-value pairs
        let expected = Porcelain::new()
            .add_section(
                "user1".to_string(),
                vec![
                    Some("name".to_string()),
                    Some("Alice".to_string()),
                    Some("age".to_string()),
                    Some("30".to_string()),
                ],
            )
            .add_section(
                "user2".to_string(),
                vec![
                    Some("name".to_string()),
                    Some("Bob".to_string()),
                    Some("age".to_string()),
                    Some("25".to_string()),
                ],
            );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_porcelain_to_rows() {
        // Test converting Porcelain to rows
        let fixture = Porcelain::new()
            .add_section(
                "user1".to_string(),
                vec![
                    Some("name".to_string()),
                    Some("Alice".to_string()),
                    Some("age".to_string()),
                    Some("30".to_string()),
                ],
            )
            .add_section(
                "user2".to_string(),
                vec![
                    Some("name".to_string()),
                    Some("Bob".to_string()),
                    Some("age".to_string()),
                    Some("25".to_string()),
                ],
            );

        let actual = fixture.to_rows();

        let expected = vec![
            vec!["user1".to_string(), "Alice".to_string(), "30".to_string()],
            vec!["user2".to_string(), "Bob".to_string(), "25".to_string()],
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_porcelain_with_missing_values() {
        // Test Porcelain with missing values (None)
        let fixture = Info::new()
            .add_title("user1")
            .add_key_value("name", "Alice")
            .add_key_value("age", "30")
            .add_title("user2")
            .add_key_value("age", "25"); // Missing name

        let porcelain: Porcelain = fixture.into();
        let actual = porcelain.to_rows();

        // user2 should have empty string for missing name
        let expected = vec![
            vec!["user1".to_string(), "Alice".to_string(), "30".to_string()],
            vec!["user2".to_string(), "".to_string(), "25".to_string()],
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_porcelain_with_different_field_order() {
        // Test that Porcelain maintains consistent field order across sections
        let fixture = Info::new()
            .add_title("user1")
            .add_key_value("name", "Alice")
            .add_key_value("age", "30")
            .add_key_value("city", "NYC")
            .add_title("user2")
            .add_key_value("age", "25") // Different order
            .add_key_value("name", "Bob");

        let porcelain: Porcelain = fixture.into();
        let actual = porcelain.to_rows();

        // All rows should have same column order: [title, name, age, city]
        let expected = vec![
            vec![
                "user1".to_string(),
                "Alice".to_string(),
                "30".to_string(),
                "NYC".to_string(),
            ],
            vec![
                "user2".to_string(),
                "Bob".to_string(),
                "25".to_string(),
                "".to_string(),
            ],
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_porcelain_empty() {
        // Test empty Porcelain
        let fixture = Porcelain::new();

        let actual = fixture.to_rows();
        let expected: Vec<Vec<String>> = vec![];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_porcelain_skip() {
        // Test skipping sections
        let fixture = Porcelain::new()
            .add_section("section1".to_string(), vec![])
            .add_section(
                "section2".to_string(),
                vec![Some("name".to_string()), Some("Alice".to_string())],
            )
            .add_section(
                "section3".to_string(),
                vec![Some("age".to_string()), Some("30".to_string())],
            );

        let actual = fixture.skip(1).to_rows();

        // Should skip section1
        let expected = vec![
            vec!["section2".to_string(), "Alice".to_string()],
            vec!["section3".to_string(), "30".to_string()],
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_porcelain_skip_more_than_available() {
        // Test skipping more sections than available
        let fixture = Porcelain::new().add_section(
            "section1".to_string(),
            vec![Some("name".to_string()), Some("Alice".to_string())],
        );

        let actual = fixture.skip(5).to_rows(); // Skip more than exists

        // Should return all sections since we can't skip more than exists
        let expected = vec![vec!["section1".to_string(), "Alice".to_string()]];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_info_to_rows() {
        // Test converting Info to rows without titles
        let fixture = Info::new()
            .add_key_value("name", "Alice")
            .add_key_value("age", "30")
            .add_key("city"); // Key-only item

        let actual = Porcelain::from(&fixture).to_rows();

        let expected = vec![
            vec!["name".to_string(), "Alice".to_string()],
            vec!["age".to_string(), "30".to_string()],
            vec!["city".to_string()],
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_info_command_structure() {
        // Test the actual structure of the info command
        // Multiple titles with different schemas - should be flat
        let fixture = Info::new()
            .add_title("PATHS")
            .add_key_value("Logs", "~/forge/logs")
            .add_key_value("Agents", "~/forge/agents")
            .add_key_value("History", "~/forge/.forge_history")
            .add_key_value("Checkpoints", "~/forge/snapshots")
            .add_key_value("Policies", "~/forge/permissions.yaml")
            .add_title("ENVIRONMENT")
            .add_key_value("Version", "0.1.0")
            .add_key_value("Working Directory", "~/code-forge")
            .add_key_value("Shell", "/bin/zsh")
            .add_key_value("Git Branch", "main")
            .add_title("CONVERSATION")
            .add_key_value("ID", "f266080c-fec6-426b-914e-178acc39483f")
            .add_title("TOKEN USAGE")
            .add_key_value("Total", "49,701")
            .add_key_value("Input", "49,440")
            .add_key_value("Cached", "116 [99%]")
            .add_key_value("Output", "261")
            .add_key_value("Cost", "$0.0176")
            .add_title("AGENT")
            .add_key_value("Name", "FORGE")
            .add_key_value("Provider", "anthropic")
            .add_key_value("Model", "claude-sonnet-4.5")
            .add_key_value("Endpoint", "https://openrouter.ai/api/v1/chat/completions")
            .add_key_value("API Key", "sk-or-v1-31eb...4631");

        let actual = Porcelain::from(&fixture).to_rows();

        // Should be flat key-value pairs without category headers
        let expected = vec![
            vec!["Logs".to_string(), "~/forge/logs".to_string()],
            vec!["Agents".to_string(), "~/forge/agents".to_string()],
            vec!["History".to_string(), "~/forge/.forge_history".to_string()],
            vec!["Checkpoints".to_string(), "~/forge/snapshots".to_string()],
            vec![
                "Policies".to_string(),
                "~/forge/permissions.yaml".to_string(),
            ],
            vec!["Version".to_string(), "0.1.0".to_string()],
            vec!["Working Directory".to_string(), "~/code-forge".to_string()],
            vec!["Shell".to_string(), "/bin/zsh".to_string()],
            vec!["Git Branch".to_string(), "main".to_string()],
            vec![
                "ID".to_string(),
                "f266080c-fec6-426b-914e-178acc39483f".to_string(),
            ],
            vec!["Total".to_string(), "49,701".to_string()],
            vec!["Input".to_string(), "49,440".to_string()],
            vec!["Cached".to_string(), "116 [99%]".to_string()],
            vec!["Output".to_string(), "261".to_string()],
            vec!["Cost".to_string(), "$0.0176".to_string()],
            vec!["Name".to_string(), "FORGE".to_string()],
            vec!["Provider".to_string(), "anthropic".to_string()],
            vec!["Model".to_string(), "claude-sonnet-4.5".to_string()],
            vec![
                "Endpoint".to_string(),
                "https://openrouter.ai/api/v1/chat/completions".to_string(),
            ],
            vec!["API Key".to_string(), "sk-or-v1-31eb...4631".to_string()],
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_tools_command_structure() {
        // Test the actual structure of the tools command
        // Multiple titles (categories) with many key-only items - should be flat
        let fixture = Info::new()
            .add_title("TOOLS")
            .add_key("[✓] read")
            .add_key("[✓] write")
            .add_key("[✓] search")
            .add_key("[✓] remove")
            .add_key("[✓] patch")
            .add_key("[✓] undo")
            .add_key("[✓] shell")
            .add_key("[✓] fetch")
            .add_key("[ ] followup")
            .add_key("[ ] plan")
            .add_key("[ ] muse")
            .add_key("[ ] forge")
            .add_key("[✓] sage")
            .add_title("MCP TOOLS")
            .add_key("[✓] mcp_deepwiki_tool_read_wiki_contents")
            .add_key("[✓] mcp_deepwiki_tool_ask_question")
            .add_key("[✓] mcp_context7_tool_get_library_docs");

        let actual = Porcelain::from(&fixture).to_rows();

        // Should be flat list of tools without category headers
        let expected = vec![
            vec!["[✓] read".to_string()],
            vec!["[✓] write".to_string()],
            vec!["[✓] search".to_string()],
            vec!["[✓] remove".to_string()],
            vec!["[✓] patch".to_string()],
            vec!["[✓] undo".to_string()],
            vec!["[✓] shell".to_string()],
            vec!["[✓] fetch".to_string()],
            vec!["[ ] followup".to_string()],
            vec!["[ ] plan".to_string()],
            vec!["[ ] muse".to_string()],
            vec!["[ ] forge".to_string()],
            vec!["[✓] sage".to_string()],
            vec!["[✓] mcp_deepwiki_tool_read_wiki_contents".to_string()],
            vec!["[✓] mcp_deepwiki_tool_ask_question".to_string()],
            vec!["[✓] mcp_context7_tool_get_library_docs".to_string()],
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_single_provider_structure() {
        // Test single provider (should be hierarchical even with 1 section)
        let fixture = Info::new()
            .add_title("PROVIDERS")
            .add_title("OpenRouter")
            .add_key_value("Domain", "[openrouter.ai]");

        let actual = Porcelain::from(&fixture).skip(1).to_rows();

        // Should be hierarchical table with provider name as first column
        let expected = vec![vec![
            "OpenRouter".to_string(),
            "[openrouter.ai]".to_string(),
        ]];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_multiple_providers_structure() {
        // Test multiple providers (hierarchical table)
        let fixture = Info::new()
            .add_title("PROVIDERS")
            .add_title("OpenRouter")
            .add_key_value("Domain", "[openrouter.ai]")
            .add_title("Anthropic")
            .add_key_value("Domain", "[api.anthropic.com]");

        let actual = Porcelain::from(&fixture).skip(1).to_rows();

        // Should be hierarchical table
        let expected = vec![
            vec!["OpenRouter".to_string(), "[openrouter.ai]".to_string()],
            vec!["Anthropic".to_string(), "[api.anthropic.com]".to_string()],
        ];

        assert_eq!(actual, expected);
    }
}
