use std::collections::{HashMap, HashSet};
use std::hash::Hash;

/// Trait for types that can be compared by a key for diffing
pub trait Diffable<Rhs = Self> {
    type Key: Hash + Eq;

    /// Returns the key used for matching items across collections
    fn key(&self) -> Self::Key;

    /// Checks if two items with the same key have equal content
    fn equals(&self, other: &Rhs) -> bool;
}

/// Result of computing differences between two collections
#[derive(Debug)]
pub struct DiffResult<'a, L, R = L> {
    /// Items only in right collection (not in left)
    pub only_right: Vec<&'a R>,

    /// Items in both collections with same key but different content
    pub modified: Vec<(&'a L, &'a R)>,

    /// Items only in left collection (not in right)
    pub only_left: Vec<&'a L>,
}

impl<'a, L, R> DiffResult<'a, L, R>
where
    L: Diffable<R>,
    R: Diffable<Key = L::Key>,
{
    /// Computes the diff between left and right collections
    ///
    /// # Arguments
    /// * `left` - The left collection to compare
    /// * `right` - The right collection to compare
    pub fn compute(left: &'a [L], right: &'a [R]) -> Self {
        let only_right = Self::find_only_right(right, left);
        let modified = Self::find_modified(right, left);
        let only_left = Self::find_only_left(right, left);

        Self { only_right, modified, only_left }
    }

    /// Check if there are any differences between collections
    pub fn has_differences(&self) -> bool {
        !self.only_right.is_empty() || !self.only_left.is_empty() || !self.modified.is_empty()
    }

    fn find_only_right(right: &'a [R], left: &[L]) -> Vec<&'a R> {
        let left_set: HashSet<_> = left.iter().map(|item| item.key()).collect();

        right
            .iter()
            .filter(|item| !left_set.contains(&item.key()))
            .collect()
    }

    fn find_modified(right: &'a [R], left: &'a [L]) -> Vec<(&'a L, &'a R)> {
        let right_map: HashMap<_, _> = right.iter().map(|item| (item.key(), item)).collect();

        left.iter()
            .filter_map(|left_item| {
                right_map.get(&left_item.key()).and_then(|right_item| {
                    if !left_item.content_equals(right_item) {
                        Some((left_item, *right_item))
                    } else {
                        None
                    }
                })
            })
            .collect()
    }

    fn find_only_left(right: &'a [R], left: &'a [L]) -> Vec<&'a L> {
        let right_set: HashSet<_> = right.iter().map(|item| item.key()).collect();

        left.iter()
            .filter(|item| !right_set.contains(&item.key()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct Item {
        id: String,
        hash: String,
    }

    impl Item {
        fn new(id: &str, hash: &str) -> Self {
            Self { id: id.to_string(), hash: hash.to_string() }
        }
    }

    impl Diffable for Item {
        type Key = String;

        fn key(&self) -> Self::Key {
            self.id.clone()
        }

        fn content_equals(&self, other: &Self) -> bool {
            // Compare both id and hash for equality
            self.id == other.id && self.hash == other.hash
        }
    }

    #[test]
    fn test_no_differences() {
        let left = vec![Item::new("a", "h1"), Item::new("b", "h2")];
        let right = vec![Item::new("a", "h1"), Item::new("b", "h2")];

        let actual = DiffResult::compute(&left, &right);

        assert!(actual.modified.is_empty());
        assert!(actual.only_left.is_empty());
        assert!(actual.only_right.is_empty());
        assert!(!actual.has_differences());
    }

    #[test]
    fn test_only_right_items() {
        let left = vec![Item::new("a", "h1")];
        let right = vec![Item::new("a", "h1"), Item::new("b", "h2")];

        let actual = DiffResult::compute(&left, &right);

        assert_eq!(actual.only_right.len(), 1);
        assert_eq!(actual.only_right[0].id, "b");
        assert!(actual.modified.is_empty());
        assert!(actual.only_left.is_empty());
    }

    #[test]
    fn test_modified_items_with_different_content() {
        let left = vec![Item::new("a", "new_hash"), Item::new("b", "h2")];
        let right = vec![Item::new("a", "old_hash"), Item::new("b", "h2")];

        let actual = DiffResult::compute(&left, &right);

        assert_eq!(actual.modified.len(), 1);
        assert!(actual.only_left.is_empty());
        assert!(actual.only_right.is_empty());
        assert!(actual.has_differences()); // modified items count as differences

        // Check the modified item
        let (left_a, right_a) = &actual.modified[0];
        assert_eq!(left_a.id, "a");
        assert_eq!(left_a.hash, "new_hash");
        assert_eq!(right_a.hash, "old_hash");
    }

    #[test]
    fn test_only_left_items() {
        let left = vec![Item::new("a", "h1"), Item::new("b", "h2")];
        let right = vec![Item::new("a", "h1")];

        let actual = DiffResult::compute(&left, &right);

        assert_eq!(actual.only_left.len(), 1);
        assert_eq!(actual.only_left[0].id, "b");
        assert!(actual.modified.is_empty());
    }

    #[test]
    fn test_complex_diff() {
        let left = vec![
            Item::new("unchanged", "h1"),
            Item::new("modified", "new_h"),
            Item::new("new", "h3"),
        ];
        let right = vec![
            Item::new("unchanged", "h1"),
            Item::new("modified", "old_h"),
            Item::new("deleted", "h4"),
        ];

        let actual = DiffResult::compute(&left, &right);

        assert_eq!(actual.only_left.len(), 1); // "new"
        assert_eq!(actual.only_right.len(), 1); // "deleted"
        assert_eq!(actual.modified.len(), 1); // "modified"
        assert!(actual.has_differences());
    }

    #[test]
    fn test_empty_collections() {
        let left: Vec<Item> = vec![];
        let right: Vec<Item> = vec![];

        let actual = DiffResult::compute(&left, &right);

        assert!(actual.modified.is_empty());
        assert!(!actual.has_differences());
    }
}
