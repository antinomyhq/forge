/// Simple history item for command storage
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryItem {
    pub item: String,
}

impl HistoryItem {
    pub fn new(command: String) -> Self {
        Self { item: command }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_history_item_creation() {
        let fixture = "test command".to_string();
        let actual = HistoryItem::new(fixture.clone());
        let expected = HistoryItem { item: fixture };
        assert_eq!(actual, expected);
    }
}
