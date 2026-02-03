use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Unique correlation identifier used to match RPC requests and responses.
///
/// Correlation IDs are carried *in-band* inside protocol envelopes.
/// They are opaque to the transport layer.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CorrelationId(String);

impl CorrelationId {
    /// Generate a new unique correlation ID.
    pub fn generate() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Borrow the correlation ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for CorrelationId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for CorrelationId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    // ---
    use super::*;

    #[test]
    fn test_generate_unique() {
        // ---
        let id1 = CorrelationId::generate();
        let id2 = CorrelationId::generate();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_format() {
        // ---
        let id = CorrelationId::generate();
        let s = id.to_string();
        assert_eq!(s.len(), 36); // Standard UUID format
    }
}
