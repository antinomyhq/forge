use std::path::PathBuf;

use anyhow::Result;

use crate::history::{FileBackedHistory, HistoryItem};

/// History search state for recursive search functionality
#[derive(Debug, Default, Clone)]
pub struct HistorySearchState {
    /// Whether search mode is active
    pub is_active: bool,
    /// Current search query
    pub query: String,
    /// Navigation state for query
    pub navigation: NavigationState<HistoryItem>,
}

/// Generic navigation state for handling collections with current index
/// tracking
#[derive(Debug, Clone)]
pub struct NavigationState<T> {
    pub matches: Vec<T>,
    pub current_index: usize,
}

impl<T> Default for NavigationState<T> {
    fn default() -> Self {
        Self { matches: Vec::new(), current_index: 0 }
    }
}

impl<T> NavigationState<T> {
    pub fn new(matches: Vec<T>) -> Self {
        Self { matches, current_index: 0 }
    }

    pub fn current(&self) -> Option<&T> {
        self.matches.get(self.current_index)
    }

    pub fn next(&mut self) -> Option<&T> {
        if self.current_index < self.matches.len().saturating_sub(1) {
            self.current_index += 1;
            self.current()
        } else {
            None
        }
    }

    pub fn previous(&mut self) -> Option<&T> {
        if self.current_index > 0 {
            self.current_index -= 1;
            self.current()
        } else {
            None
        }
    }

    pub fn reset(&mut self) {
        self.current_index = 0;
        self.matches.clear();
    }
}

impl HistorySearchState {
    /// Enter search mode with an optional initial query
    pub fn enter_search(&mut self, initial_query: Option<String>) {
        self.is_active = true;
        self.query = initial_query.unwrap_or_default();
        self.navigation.reset();
    }

    /// Exit search mode and reset state
    pub fn exit_search(&mut self) {
        self.is_active = false;
        self.query.clear();
        self.navigation.reset();
    }

    /// Update search query and reset match index
    pub fn update_query(&mut self, query: String) {
        self.query = query;
        self.navigation.reset();
    }

    /// Navigate to next match
    pub fn next_match(&mut self) {
        self.navigation.next();
    }

    /// Navigate to previous match
    pub fn prev_match(&mut self) {
        self.navigation.previous();
    }

    /// Get current match if available
    pub fn current_match(&self) -> Option<String> {
        self.navigation.current().map(|item| item.item.clone())
    }

    /// Update search results based on current query
    pub fn update_search(&mut self, history: &History) {
        self.navigation = NavigationState::new(history.search(&self.query));
    }

    /// Get search status string for display
    pub fn status_text(&self) -> String {
        if self.navigation.matches.is_empty() {
            if self.query.is_empty() {
                "reverse-i-search: ".to_string()
            } else {
                format!("failing reverse-i-search: {}", self.query)
            }
        } else {
            format!(
                "reverse-i-search: {} [{}/{}]",
                self.query,
                self.navigation.current_index + 1,
                self.navigation.matches.len()
            )
        }
    }
}

/// Command history wrapper around file-backed history with navigation tracking
#[derive(Debug)]
pub struct History {
    /// File-backed history implementation
    history: FileBackedHistory,
    /// Navigation state for history items
    navigation: NavigationState<HistoryItem>,
}

impl History {
    /// Create a new History with a specified max size and file path
    pub fn with_file(max_size: usize, path: PathBuf) -> Result<Self> {
        let history = FileBackedHistory::with_file(max_size, path)?;
        Ok(Self { history, navigation: NavigationState::default() })
    }

    /// Add a command to history with LRU behavior and file persistence
    pub fn add_command(&mut self, command: String) -> Result<()> {
        self.history.save(HistoryItem::new(command))?;
        self.navigation.reset();
        Ok(())
    }

    /// Navigate to previous command (up arrow)
    pub fn navigate_up(&mut self) -> Option<String> {
        self.navigation.next().map(|item| item.item.clone())
    }

    /// Navigate to next command (down arrow)
    pub fn navigate_down(&mut self) -> Option<String> {
        self.navigation.previous().map(|item| item.item.clone())
    }

    /// Get autocomplete suggestion for current input
    pub fn prefix_search(&mut self, current_input: &str) -> Option<String> {
        if current_input.trim().is_empty() {
            return None;
        }

        let results = self
            .history
            .search_prefix(current_input)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();

        self.navigation = NavigationState::new(results.clone());
        results.first().map(|item| item.item.clone())
    }

    /// Reset navigation state (called when user types)
    pub fn reset_navigation(&mut self) {
        self.navigation.reset();
    }

    /// Get the number of commands in history
    pub fn len(&self) -> usize {
        self.history.total_entries()
    }

    /// Check if history is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Flush history to file
    pub fn flush(&mut self) -> Result<()> {
        self.history.sync().map_err(|e| anyhow::anyhow!(e))
    }

    /// Search history with a query
    pub fn search(&self, query: &str) -> Vec<HistoryItem> {
        if query.trim().is_empty() {
            return Vec::new();
        }

        self.history
            .get_all_items()
            .into_iter()
            .filter(|item| item.item.contains(query))
            .cloned()
            .collect()
    }

    /// Get all commands for search (most recent first)
    pub fn get_all_commands(&self) -> Vec<String> {
        self.history
            .get_all_items()
            .iter()
            .map(|item| item.item.clone())
            .collect()
    }
}
