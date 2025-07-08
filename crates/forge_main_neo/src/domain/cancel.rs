/// A unique identifier for cancellation operations
///
/// CancelId is a simple wrapper around a u64 that provides a type-safe
/// way to identify cancellation requests. The executor maintains an internal
/// mapping between CancelId and CancellationToken instances.
///
/// # Examples
///
/// ```rust
/// use forge_main_neo::domain::CancelId;
///
/// let cancel_id = CancelId::new(42);
/// assert_eq!(cancel_id.id(), 42);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CancelId {
    id: u64,
}

impl CancelId {
    /// Create a new CancelId with the given numeric identifier
    pub fn new(id: u64) -> Self {
        Self { id }
    }

    /// Get the numeric identifier
    pub fn id(&self) -> u64 {
        self.id
    }
}

impl From<u64> for CancelId {
    fn from(id: u64) -> Self {
        Self::new(id)
    }
}

impl From<CancelId> for u64 {
    fn from(cancel_id: CancelId) -> Self {
        cancel_id.id
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_cancel_id_new() {
        let cancel_id = CancelId::new(42);
        let actual = cancel_id.id();
        let expected = 42;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_cancel_id_from_u64() {
        let cancel_id = CancelId::from(123);
        let actual = cancel_id.id();
        let expected = 123;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_cancel_id_into_u64() {
        let cancel_id = CancelId::new(456);
        let actual: u64 = cancel_id.into();
        let expected = 456;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_cancel_id_equality() {
        let cancel_id1 = CancelId::new(789);
        let cancel_id2 = CancelId::new(789);
        let cancel_id3 = CancelId::new(999);

        assert_eq!(cancel_id1, cancel_id2);
        assert!(cancel_id1 != cancel_id3);
    }

    #[test]
    fn test_cancel_id_hash() {
        use std::collections::HashMap;

        let cancel_id1 = CancelId::new(111);
        let cancel_id2 = CancelId::new(222);

        let mut map = HashMap::new();
        map.insert(cancel_id1, "first");
        map.insert(cancel_id2, "second");

        let actual1 = map.get(&cancel_id1);
        let actual2 = map.get(&cancel_id2);

        assert_eq!(actual1, Some(&"first"));
        assert_eq!(actual2, Some(&"second"));
    }

    #[test]
    fn test_cancel_id_debug() {
        let cancel_id = CancelId::new(333);
        let actual = format!("{:?}", cancel_id);
        let expected = "CancelId { id: 333 }";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_cancel_id_clone() {
        let cancel_id1 = CancelId::new(444);
        let cancel_id2 = cancel_id1.clone();

        assert_eq!(cancel_id1, cancel_id2);
        assert_eq!(cancel_id1.id(), cancel_id2.id());
    }

    #[test]
    fn test_cancel_id_copy() {
        let cancel_id1 = CancelId::new(555);
        let cancel_id2 = cancel_id1; // Copy, not move

        // Both should still be usable
        assert_eq!(cancel_id1.id(), 555);
        assert_eq!(cancel_id2.id(), 555);
    }
}
