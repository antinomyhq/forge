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
    pub fn skip(self, n: usize) -> Self {
        Porcelain(self.0.into_iter().skip(n).collect())
    }

    #[allow(unused)]
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

    /// Maps a function over all cells in the specified column
    pub fn map_col<F>(self, c: usize, f: F) -> Self
    where
        F: Fn(Option<String>) -> Option<String>,
    {
        Porcelain(
            self.0
                .into_iter()
                .map(|row| {
                    row.into_iter()
                        .enumerate()
                        .map(|(i, col)| if i == c { f(col) } else { col })
                        .collect()
                })
                .collect(),
        )
    }

    /// Truncates the specified column to a maximum length (including "..."),
    /// appending "..." if truncated
    pub fn truncate(self, c: usize, max_len: usize) -> Self {
        self.map_col(c, |col| {
            col.map(|value| {
                if value.len() > max_len {
                    format!("{}...", &value[..max_len.saturating_sub(3)])
                } else {
                    value
                }
            })
        })
    }
    #[allow(unused)]
    pub fn into_body(self) -> Vec<Vec<Option<String>>> {
        // Skip headers and return
        self.0.into_iter().skip(1).collect()
    }

    #[allow(unused)]
    pub fn into_rows(self) -> Vec<Vec<Option<String>>> {
        self.0
    }

    /// Converts from wide format to long format.
    ///
    /// Transforms entity-centric rows (wide format with many columns) into
    /// field-centric rows (long format with three columns: entity_id,
    /// field_name, field_value). This is also known as unpivoting or
    /// melting in data transformation terminology.
    ///
    /// # Example
    /// Input (wide format):
    /// ```text
    /// Headers: [$ID, version, shell, id, title, model]
    /// Row 1:   [env, 0.1.0,   zsh,   None, None, None]
    /// Row 2:   [conversation, None, None, 000-000-000, make agents great again, None]
    /// ```
    ///
    /// Output (long format):
    /// ```text
    /// Headers: [$ID, field, value]
    /// Row 1:   [env, version, 0.1.0]
    /// Row 2:   [env, shell, zsh]
    /// Row 3:   [conversation, id, 000-000-000]
    /// Row 4:   [conversation, title, make agents great again]
    /// ```
    pub fn into_long(self) -> Self {
        if self.0.is_empty() {
            return self;
        }

        let headers = &self.0[0];
        let data_rows = &self.0[1..];

        if data_rows.is_empty() || headers.is_empty() {
            return self;
        }

        // Create new headers: [$ID, $FIELD, $VALUE]
        let new_headers = vec![
            headers.first().cloned().unwrap_or(Some("$ID".to_string())),
            Some("$FIELD".to_string()),
            Some("$VALUE".to_string()),
        ];

        // Create new rows: one row per non-None field for each entity
        let mut new_rows = Vec::new();

        for data_row in data_rows {
            // Get the entity ID (first column value)
            let entity_id = data_row.first().and_then(|v| v.clone());

            // For each field in this entity (excluding $ID column)
            for (i, value) in data_row.iter().enumerate().skip(1) {
                if let Some(value) = value {
                    let field_name = headers.get(i).and_then(|h| h.clone());
                    new_rows.push(vec![entity_id.clone(), field_name, Some(value.to_owned())]);
                }
            }
        }

        let mut result = vec![new_headers];
        result.extend(new_rows);

        Porcelain(result)
    }
}

impl fmt::Display for Porcelain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            return Ok(());
        }

        // Calculate the maximum width for each column
        let num_cols = self.0.iter().map(|row| row.len()).max().unwrap_or(0);
        let mut col_widths = vec![0; num_cols];

        for row in &self.0 {
            for (i, cell) in row.iter().enumerate() {
                let width = cell.as_ref().map(|s| s.len()).unwrap_or(0);
                col_widths[i] = col_widths[i].max(width);
            }
        }

        // Format each row
        let mut lines = Vec::new();
        for row in &self.0 {
            let mut line = String::new();
            for (i, cell) in row.iter().enumerate() {
                let content = cell.as_ref().map(|s| s.as_str()).unwrap_or("");

                if i == row.len() - 1 {
                    // Last column: no padding
                    line.push_str(content);
                } else {
                    // Pad to column width
                    line.push_str(&format!("{:<width$}", content, width = col_widths[i]));
                    line.push_str("  ");
                }
            }
            lines.push(line);
        }

        write!(f, "{}", lines.join("\n"))
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
                    cells.insert("$ID".to_owned(), Some(title.to_owned()));
                    keys.insert("$ID".to_owned());
                }
                Section::Items(key, value) => {
                    let default_key = format!("$VALUE_{}", cells.len());
                    let key = key.clone().unwrap_or(default_key);
                    cells.insert(key.clone(), Some(value.clone()));
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
                .map(|key| rows.get(key).and_then(|value| value.as_ref().cloned()))
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

        let actual = info.drop_col(1).into_rows();

        let expected = vec![
            vec![Some("user1".into()), Some("30".into())],
            vec![Some("user2".into()), Some("25".into())],
        ];

        assert_eq!(actual, expected)
    }

    #[test]
    fn test_map_col() {
        let info = Porcelain(vec![
            vec![
                Some("user1".into()),
                Some("Alice".into()),
                Some("30".into()),
            ],
            vec![Some("user2".into()), Some("Bob".into()), Some("25".into())],
        ]);

        let actual = info
            .map_col(1, |col| col.map(|v| v.to_uppercase()))
            .into_rows();

        let expected = vec![
            vec![
                Some("user1".into()),
                Some("ALICE".into()),
                Some("30".into()),
            ],
            vec![Some("user2".into()), Some("BOB".into()), Some("25".into())],
        ];

        assert_eq!(actual, expected)
    }
    #[test]
    fn test_truncate() {
        let info = Porcelain(vec![
            vec![
                Some("user1".into()),
                Some("Alice".into()),
                Some("very_long_name".into()),
            ],
            vec![
                Some("user2".into()),
                Some("Bob".into()),
                Some("short".into()),
            ],
        ]);

        let actual = info.truncate(2, 5).into_rows();

        let expected = vec![
            vec![
                Some("user1".into()),
                Some("Alice".into()),
                Some("ve...".into()),
            ],
            vec![
                Some("user2".into()),
                Some("Bob".into()),
                Some("short".into()),
            ],
        ];

        assert_eq!(actual, expected)
    }

    #[test]
    fn test_into_long() {
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

        let actual = Porcelain::from(info).into_long();
        let expected = vec![
            vec![
                Some("env".into()),
                Some("version".into()),
                Some("0.1.0".into()),
            ],
            vec![Some("env".into()), Some("shell".into()), Some("zsh".into())],
            vec![
                Some("conversation".into()),
                Some("id".into()),
                Some("000-000-000".into()),
            ],
            vec![
                Some("conversation".into()),
                Some("title".into()),
                Some("make agents great again".into()),
            ],
            vec![
                Some("agent".into()),
                Some("id".into()),
                Some("forge".into()),
            ],
            vec![
                Some("agent".into()),
                Some("model".into()),
                Some("sonnet-4".into()),
            ],
        ];

        assert_eq!(actual.into_body(), expected)
    }

    #[test]
    fn test_from_info_single_col() {
        let info = Info::new()
            .add_title("T1")
            .add_value("a1")
            .add_value("b1")
            .add_title("T2")
            .add_value("a2")
            .add_value("b2")
            .add_title("T3")
            .add_value("a3")
            .add_value("b3");

        let actual = Porcelain::from(info).into_rows();

        let expected = vec![
            //
            vec![
                Some("$ID".into()),
                Some("$VALUE_1".into()),
                Some("$VALUE_2".into()),
            ],
            vec![Some("T1".into()), Some("a1".into()), Some("b1".into())],
            vec![Some("T2".into()), Some("a2".into()), Some("b2".into())],
            vec![Some("T3".into()), Some("a3".into()), Some("b3".into())],
        ];

        assert_eq!(actual, expected)
    }

    #[test]
    fn test_into_long_single_col() {
        let info = Info::new()
            .add_title("T1")
            .add_value("a1")
            .add_value("b1")
            .add_title("T2")
            .add_value("a2")
            .add_value("b2")
            .add_title("T3")
            .add_value("a3")
            .add_value("b3");

        let actual = Porcelain::from(info).into_long().into_rows();

        let expected = vec![
            vec![
                Some("$ID".into()),
                Some("$FIELD".into()),
                Some("$VALUE".into()),
            ],
            vec![
                Some("T1".into()),
                Some("$VALUE_1".into()),
                Some("a1".into()),
            ],
            vec![
                Some("T1".into()),
                Some("$VALUE_2".into()),
                Some("b1".into()),
            ],
            vec![
                Some("T2".into()),
                Some("$VALUE_1".into()),
                Some("a2".into()),
            ],
            vec![
                Some("T2".into()),
                Some("$VALUE_2".into()),
                Some("b2".into()),
            ],
            vec![
                Some("T3".into()),
                Some("$VALUE_1".into()),
                Some("a3".into()),
            ],
            vec![
                Some("T3".into()),
                Some("$VALUE_2".into()),
                Some("b3".into()),
            ],
        ];

        assert_eq!(actual, expected)
    }

    #[test]
    fn test_display_simple() {
        let info = Porcelain(vec![
            vec![Some("$ID".into()), Some("name".into()), Some("age".into())],
            vec![
                Some("user1".into()),
                Some("Alice".into()),
                Some("30".into()),
            ],
            vec![Some("user2".into()), Some("Bob".into()), Some("25".into())],
        ]);

        let actual = info.to_string();
        let expected = [
            //
            "$ID    name   age",
            "user1  Alice  30",
            "user2  Bob    25",
        ]
        .join("\n");

        assert_eq!(actual, expected)
    }

    #[test]
    fn test_display_with_none() {
        let info = Porcelain(vec![
            vec![Some("$ID".into()), Some("name".into()), Some("age".into())],
            vec![
                Some("user1".into()),
                Some("Alice".into()),
                Some("30".into()),
            ],
            vec![Some("user2".into()), None, Some("25".into())],
        ]);

        let actual = info.to_string();
        let expected = [
            //
            "$ID    name   age",
            "user1  Alice  30",
            "user2         25",
        ]
        .join("\n");

        assert_eq!(actual, expected)
    }

    #[test]
    fn test_display_empty() {
        let info = Porcelain::new();

        let actual = info.to_string();
        let expected = "";

        assert_eq!(actual, expected)
    }
}
