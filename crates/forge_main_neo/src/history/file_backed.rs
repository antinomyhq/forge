use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;

use crate::history::HistoryItem;

/// Simple file-backed history implementation
#[derive(Debug)]
pub struct FileBackedHistory {
    /// In-memory storage with LRU behavior
    items: VecDeque<HistoryItem>,
    /// Maximum capacity
    capacity: usize,
    /// File for persistence
    file: fd_lock::RwLock<File>,
}

impl FileBackedHistory {
    /// Create a new file-backed history with specified capacity and file path
    pub fn with_file(capacity: usize, file_path: PathBuf) -> anyhow::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .append(true)
            .open(&file_path)?;
        let mut history = Self {
            items: VecDeque::with_capacity(capacity),
            capacity,
            file: fd_lock::RwLock::new(file),
        };
        // Load existing history from file if it exists
        history.sync_from_file()?;
        Ok(history)
    }

    /// Add a new command to history
    pub fn save(&mut self, item: HistoryItem) -> anyhow::Result<()> {
        // Don't add empty or whitespace-only commands
        let trimmed_command = item.item.trim().to_string();
        if trimmed_command.is_empty() {
            return Ok(());
        }

        let new_item = HistoryItem::new(trimmed_command);
        let is_duplicate = self
            .items
            .back()
            .map(|item| item.item == new_item.item)
            .unwrap_or(false);
        if !is_duplicate {
            // append to the file.
            let mut write_guard = self.file.write()?;
            write_guard.write_all(format!("{}\n", escape(new_item.item.as_str())).as_bytes())?;

            // add to in-memory history
            self.items.push_back(new_item);

            // Maintain capacity
            while self.items.len() > self.capacity {
                self.items.pop_front();
            }
        }

        Ok(())
    }

    /// Get command at index from most recent (0 = most recent)
    pub fn get(&self, index: usize) -> Option<&HistoryItem> {
        self.items.get(index)
    }

    /// Find commands that start with the given prefix
    pub fn search_prefix(&self, prefix: &str) -> Vec<&HistoryItem> {
        self.items
            .iter()
            .rev()
            .filter(|item| item.item.starts_with(prefix))
            .collect()
    }

    /// Sync to file (explicit flush)
    pub fn sync(&mut self) -> anyhow::Result<()> {
        self.sync_to_file()
    }

    pub fn total_entries(&self) -> usize {
        self.items.len()
    }

    /// Get all items for searching (most recent first)
    pub fn get_all_items(&self) -> Vec<&HistoryItem> {
        self.items.iter().rev().collect()
    }

    /// Load history from file
    fn sync_from_file(&mut self) -> anyhow::Result<()> {
        let mut content = String::new();
        let mut write_guard = self.file.write()?;
        write_guard.read_to_string(&mut content)?;
        drop(write_guard);

        let lines: Vec<&str> = content.lines().collect();
        let total_commands = lines.len();
        let skip_lines = total_commands.saturating_sub(self.capacity);
        for line in lines.into_iter().skip(skip_lines) {
            let line = line.trim();
            if !line.is_empty() {
                self.items.push_back(HistoryItem::new(unescape(line)));
            }
        }

        Ok(())
    }

    /// Save history to file
    fn sync_to_file(&mut self) -> anyhow::Result<()> {
        let content: String = self
            .items
            .iter()
            .map(|item| escape(item.item.as_str()))
            .collect::<Vec<_>>()
            .join("\n");

        // finally clear the file and write the new content.
        let mut write_guard = self.file.write()?;
        write_guard.set_len(0)?;
        write_guard.write_all(format!("{content}\n").as_bytes())?;
        Ok(())
    }
}

impl Drop for FileBackedHistory {
    fn drop(&mut self) {
        let _ = self.sync();
    }
}

fn escape(s: &str) -> String {
    s.replace('\n', "<\\n>")
}

fn unescape(s: &str) -> String {
    s.replace("<\\n>", "\n")
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Seek, SeekFrom};

    use pretty_assertions::assert_eq;
    use tempfile::NamedTempFile;

    use super::*;

    fn create_test_history(capacity: usize) -> (FileBackedHistory, NamedTempFile) {
        let temp_file = NamedTempFile::new().unwrap();
        let history =
            FileBackedHistory::with_file(capacity, temp_file.path().to_path_buf()).unwrap();
        (history, temp_file)
    }

    fn create_history_item(command: &str) -> HistoryItem {
        HistoryItem::new(command.to_string())
    }

    #[test]
    fn test_save_removes_duplicates() {
        let (mut fixture, _temp_file) = create_test_history(10);

        fixture.save(create_history_item("command1")).unwrap();
        fixture.save(create_history_item("command1")).unwrap(); // Duplicate
        fixture.save(create_history_item("command2")).unwrap();
        fixture.save(create_history_item("command3")).unwrap();
        let actual_total = fixture.total_entries();
        assert_eq!(actual_total, 3);
    }

    #[test]
    fn test_save_maintains_capacity_limit() {
        let (mut fixture, _temp_file) = create_test_history(2);

        fixture.save(create_history_item("command1")).unwrap();
        fixture.save(create_history_item("command2")).unwrap();
        fixture.save(create_history_item("command3")).unwrap();

        let actual_total = fixture.total_entries();
        let actual_first = fixture.get(0).unwrap().item.as_str();
        let actual_second = fixture.get(1).unwrap().item.as_str();
        let actual_third = fixture.get(2);

        let expected_total = 2;
        let expected_first = "command2";
        let expected_second = "command3";

        assert_eq!(actual_total, expected_total);
        assert_eq!(actual_first, expected_first);
        assert_eq!(actual_second, expected_second);
        assert!(actual_third.is_none());
    }

    #[test]
    fn test_search_prefix_finds_matching_commands() {
        let (mut fixture, _temp_file) = create_test_history(10);

        fixture.save(create_history_item("git status")).unwrap();
        fixture.save(create_history_item("ls -la")).unwrap();
        fixture.save(create_history_item("git commit")).unwrap();
        fixture.save(create_history_item("git push")).unwrap();

        let actual = fixture.search_prefix("git");
        let expected_count = 3;
        let expected_commands: Vec<&str> = vec!["git push", "git commit", "git status"];

        assert_eq!(actual.len(), expected_count);
        for (i, item) in actual.iter().enumerate() {
            assert_eq!(item.item.as_str(), expected_commands[i]);
        }
    }

    #[test]
    fn test_search_prefix_returns_empty_for_no_matches() {
        let (mut fixture, _temp_file) = create_test_history(10);

        fixture.save(create_history_item("git status")).unwrap();
        fixture.save(create_history_item("ls -la")).unwrap();

        let actual = fixture.search_prefix("docker");
        let expected: Vec<&HistoryItem> = vec![];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_file_persistence_on_creation() {
        let temp_file = NamedTempFile::new().unwrap();
        std::fs::write(temp_file.path(), "existing1\nexisting2\nexisting3\n").unwrap();

        let fixture = FileBackedHistory::with_file(10, temp_file.path().to_path_buf()).unwrap();

        let actual_total = fixture.total_entries();
        let actual_first = fixture.get(0).unwrap().item.as_str();
        let actual_last = fixture.get(2).unwrap().item.as_str();

        let expected_total = 3;
        let expected_first = "existing1"; // Most recent (loaded in reverse order)
        let expected_last = "existing3"; // Oldest

        assert_eq!(actual_total, expected_total);
        assert_eq!(actual_first, expected_first);
        assert_eq!(actual_last, expected_last);
    }

    #[test]
    fn test_file_persistence_respects_capacity_on_load() {
        let temp_file = NamedTempFile::new().unwrap();
        std::fs::write(temp_file.path(), "cmd1\ncmd2\ncmd3\ncmd4\ncmd5\n").unwrap();

        let fixture = FileBackedHistory::with_file(3, temp_file.path().to_path_buf()).unwrap();

        let actual_total = fixture.total_entries();
        let actual_commands: Vec<String> = (0..actual_total)
            .map(|i| fixture.get(i).unwrap().item.clone())
            .collect();

        let expected_total = 3;
        let expected_commands = vec!["cmd3".to_string(), "cmd4".to_string(), "cmd5".to_string()];

        assert_eq!(actual_total, expected_total);
        assert_eq!(actual_commands, expected_commands);
    }

    #[test]
    fn test_total_entries_returns_correct_count() {
        let (mut fixture, _temp_file) = create_test_history(10);

        assert_eq!(fixture.total_entries(), 0);

        fixture.save(create_history_item("command1")).unwrap();
        assert_eq!(fixture.total_entries(), 1);

        fixture.save(create_history_item("command1")).unwrap(); // Duplicate
        assert_eq!(fixture.total_entries(), 1);

        fixture.save(create_history_item("command2")).unwrap();
        assert_eq!(fixture.total_entries(), 2);
    }

    #[test]
    fn test_sync_explicit_flush() {
        let (mut fixture, _temp_file) = create_test_history(10);

        fixture.save(create_history_item("command1")).unwrap();
        fixture.save(create_history_item("command2")).unwrap();

        let actual = fixture.sync();
        assert!(actual.is_ok());
    }
}
