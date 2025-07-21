use tokio_util::sync::CancellationToken;

/// A unique identifier for cancellation operations
#[derive(Debug, Clone)]
pub struct CancelId {
    token: CancellationToken,
    // Use a unique ID for comparison since CancellationToken doesn't implement PartialEq
    id: u64,
}

static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl CancelId {
    /// Create a new CancelId with the given CancellationToken
    pub fn new(token: CancellationToken) -> Self {
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Self { token, id }
    }

    /// Get a reference to the CancellationToken
    pub fn token(&self) -> &CancellationToken {
        &self.token
    }

    /// Get the CancellationToken by value
    pub fn into_token(self) -> CancellationToken {
        self.token
    }

    /// Cancel the operation
    pub fn cancel(&self) {
        self.token.cancel();
    }

    /// Check if the operation is cancelled
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }
}

impl From<CancellationToken> for CancelId {
    fn from(token: CancellationToken) -> Self {
        Self::new(token)
    }
}

impl From<CancelId> for CancellationToken {
    fn from(cancel_id: CancelId) -> Self {
        cancel_id.token
    }
}

impl PartialEq for CancelId {
    fn eq(&self, other: &Self) -> bool {
        // Compare by the unique ID
        self.id == other.id
    }
}

impl Eq for CancelId {}

impl std::hash::Hash for CancelId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Hash the unique ID
        self.id.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tokio_util::sync::CancellationToken;

    use super::*;

    #[test]
    fn test_cancel_id_new() {
        let token = CancellationToken::new();
        let cancel_id = CancelId::new(token.clone());
        // Just verify that the cancel_id was created successfully
        // We can't compare tokens directly since they don't implement PartialEq
        assert!(!cancel_id.is_cancelled());
    }

    #[test]
    fn test_cancel_id_from_token() {
        let token = CancellationToken::new();
        let cancel_id = CancelId::from(token.clone());
        // Just verify that the cancel_id was created successfully
        assert!(!cancel_id.is_cancelled());
    }

    #[test]
    fn test_cancel_id_into_token() {
        let token = CancellationToken::new();
        let cancel_id = CancelId::new(token.clone());
        let _actual: CancellationToken = cancel_id.into();
        // We can't compare tokens directly, but we can verify the conversion
        // worked by checking that no panic occurred
    }

    #[test]
    fn test_cancel_id_cancel() {
        let token = CancellationToken::new();
        let cancel_id = CancelId::new(token.clone());

        assert!(!cancel_id.is_cancelled());
        cancel_id.cancel();
        assert!(cancel_id.is_cancelled());
    }

    #[test]
    fn test_cancel_id_equality() {
        let token1 = CancellationToken::new();
        let token2 = CancellationToken::new();

        let cancel_id1 = CancelId::new(token1.clone());
        let cancel_id2 = cancel_id1.clone(); // Same ID
        let cancel_id3 = CancelId::new(token2); // Different ID

        assert_eq!(cancel_id1, cancel_id2);
        assert!(cancel_id1 != cancel_id3);
    }

    #[test]
    fn test_cancel_id_hash() {
        use std::collections::HashMap;

        let token1 = CancellationToken::new();
        let token2 = CancellationToken::new();

        let cancel_id1 = CancelId::new(token1);
        let cancel_id2 = CancelId::new(token2);

        let mut map = HashMap::new();
        map.insert(cancel_id1.clone(), "first");
        map.insert(cancel_id2.clone(), "second");

        let actual1 = map.get(&cancel_id1);
        let actual2 = map.get(&cancel_id2);

        assert_eq!(actual1, Some(&"first"));
        assert_eq!(actual2, Some(&"second"));
    }

    #[test]
    fn test_cancel_id_debug() {
        let token = CancellationToken::new();
        let cancel_id = CancelId::new(token);
        let actual = format!("{:?}", cancel_id);
        // Just check that it contains the expected structure
        assert!(actual.contains("CancelId"));
    }

    #[test]
    fn test_cancel_id_clone() {
        let token = CancellationToken::new();
        let cancel_id1 = CancelId::new(token);
        let cancel_id2 = cancel_id1.clone();

        assert_eq!(cancel_id1, cancel_id2);
        // We can't compare tokens directly, but we can verify both work
        assert!(!cancel_id1.is_cancelled());
        assert!(!cancel_id2.is_cancelled());
    }

    #[test]
    fn test_cancel_id_into_token_method() {
        let token = CancellationToken::new();
        let cancel_id = CancelId::new(token.clone());
        let _actual = cancel_id.into_token();
        // We can't compare tokens directly, but we can verify the conversion
        // worked by checking that no panic occurred
    }
}
